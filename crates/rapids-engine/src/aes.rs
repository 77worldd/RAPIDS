//! AES-128 in CBC mode with PKCS#7 padding, the scheme HLS AES-128
//! segments are encrypted with.
//!
//! HLS content that carries an `#EXT-X-KEY:METHOD=AES-128` line is
//! segmented, and each segment is a complete CBC ciphertext whose IV is the
//! key's IV attribute, or the segment's sequence number when the playlist
//! omits one. Decrypting therefore needs a whole cipher, not a streaming
//! mode: every segment starts from its own IV.
//!
//! Implemented directly rather than through a dependency so the crate keeps
//! its dependency set unchanged. The only operations exposed are the two the
//! HLS pipeline needs: encrypt a padded buffer, and decrypt a padded buffer
//! back to its original length.

const BLOCK: usize = 16;
const ROUNDS: usize = 10;
const KEY_LEN: usize = 16;

/// Failures of the cipher itself, before any HLS concern is involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AesError {
    /// The key was not exactly 128 bits.
    BadKeyLength,
    /// A buffer that is not a whole number of blocks was handed to CBC.
    BadLength,
    /// The padding bytes are not a valid PKCS#7 tail.
    BadPadding,
}

impl std::fmt::Display for AesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            AesError::BadKeyLength => "AES-128 key must be 16 bytes",
            AesError::BadLength => "CBC input must be a whole number of 16-byte blocks",
            AesError::BadPadding => "PKCS#7 padding is not valid",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AesError {}

/// Multiply two elements of GF(2^8) modulo the AES polynomial
/// `x^8 + x^4 + x^3 + x + 1` (`0x1b`).
fn gf_mul(mut first: u8, mut second: u8) -> u8 {
    let mut product = 0u8;
    for _ in 0..8 {
        if second & 1 != 0 {
            product ^= first;
        }
        let carry = first & 0x80;
        first <<= 1;
        if carry != 0 {
            first ^= 0x1b;
        }
        second >>= 1;
    }
    product
}

fn gf_pow(mut base: u8, mut exponent: u32) -> u8 {
    let mut result = 1u8;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = gf_mul(result, base);
        }
        base = gf_mul(base, base);
        exponent >>= 1;
    }
    result
}

/// The AES S-box. Built from the multiplicative inverse plus the affine
/// transform rather than transcribed, so the table cannot drift.
fn build_sbox() -> [u8; 256] {
    let mut sbox = [0u8; 256];
    for value in 0..=255u32 {
        let byte = value as u8;
        let inverse = if byte == 0 { 0 } else { gf_pow(byte, 254) };
        let rotated = inverse
            ^ ((inverse << 1) ^ (inverse >> 7))
            ^ ((inverse << 2) ^ (inverse >> 6))
            ^ ((inverse << 3) ^ (inverse >> 5))
            ^ ((inverse << 4) ^ (inverse >> 4));
        sbox[value as usize] = rotated ^ 0x63;
    }
    sbox
}

/// Substitute, shift the rows of the state, and mix the columns. The state
/// is a flat 16-byte block in column-major order: index `row + 4 * column`.
fn sub_bytes(sbox: &[u8; 256], state: &mut [u8; BLOCK]) {
    for byte in state.iter_mut() {
        *byte = sbox[*byte as usize];
    }
}

fn shift_rows(state: &mut [u8; BLOCK]) {
    let mut out = *state;
    for row in 1..4 {
        for column in 0..4 {
            let source = (column + row) % 4;
            out[row + 4 * column] = state[row + 4 * source];
        }
    }
    *state = out;
}

fn inv_shift_rows(state: &mut [u8; BLOCK]) {
    let mut out = *state;
    for row in 1..4 {
        for column in 0..4 {
            let source = (column + 4 - row) % 4;
            out[row + 4 * column] = state[row + 4 * source];
        }
    }
    *state = out;
}

fn mix_columns(state: &mut [u8; BLOCK]) {
    for column in 0..4 {
        let base = column * 4;
        let first = state[base];
        let second = state[base + 1];
        let third = state[base + 2];
        let fourth = state[base + 3];
        state[base] = gf_mul(first, 2) ^ gf_mul(second, 3) ^ third ^ fourth;
        state[base + 1] = first ^ gf_mul(second, 2) ^ gf_mul(third, 3) ^ fourth;
        state[base + 2] = first ^ second ^ gf_mul(third, 2) ^ gf_mul(fourth, 3);
        state[base + 3] = gf_mul(first, 3) ^ second ^ third ^ gf_mul(fourth, 2);
    }
}

fn inv_mix_columns(state: &mut [u8; BLOCK]) {
    for column in 0..4 {
        let base = column * 4;
        let first = state[base];
        let second = state[base + 1];
        let third = state[base + 2];
        let fourth = state[base + 3];
        state[base] =
            gf_mul(first, 14) ^ gf_mul(second, 11) ^ gf_mul(third, 13) ^ gf_mul(fourth, 9);
        state[base + 1] =
            gf_mul(first, 9) ^ gf_mul(second, 14) ^ gf_mul(third, 11) ^ gf_mul(fourth, 13);
        state[base + 2] =
            gf_mul(first, 13) ^ gf_mul(second, 9) ^ gf_mul(third, 14) ^ gf_mul(fourth, 11);
        state[base + 3] =
            gf_mul(first, 11) ^ gf_mul(second, 13) ^ gf_mul(third, 9) ^ gf_mul(fourth, 14);
    }
}

fn expand_key(key: &[u8], sbox: &[u8; 256]) -> [[u8; BLOCK]; ROUNDS + 1] {
    let mut words = [[0u8; 4]; 4 * (ROUNDS + 1)];
    for word in 0..4 {
        for byte in 0..4 {
            words[word][byte] = key[word * 4 + byte];
        }
    }
    let rcon: [u8; ROUNDS] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];
    for index in 4..words.len() {
        let mut temp = words[index - 1];
        if index % 4 == 0 {
            temp = [temp[1], temp[2], temp[3], temp[0]];
            for byte in temp.iter_mut() {
                *byte = sbox[*byte as usize];
            }
            temp[0] ^= rcon[index / 4 - 1];
        }
        for byte in 0..4 {
            words[index][byte] = words[index - 4][byte] ^ temp[byte];
        }
    }

    let mut round_keys = [[0u8; BLOCK]; ROUNDS + 1];
    for round in 0..=ROUNDS {
        for word in 0..4 {
            for byte in 0..4 {
                round_keys[round][word * 4 + byte] = words[round * 4 + word][byte];
            }
        }
    }
    round_keys
}

/// AES-128 with its expanded key schedule, ready to process blocks.
#[derive(Clone)]
pub struct Aes128 {
    sbox: [u8; 256],
    inv_sbox: [u8; 256],
    round_keys: [[u8; BLOCK]; ROUNDS + 1],
}

impl Aes128 {
    /// Builds the cipher from a 128-bit key.
    pub fn new(key: &[u8]) -> Result<Self, AesError> {
        if key.len() != KEY_LEN {
            return Err(AesError::BadKeyLength);
        }
        let sbox = build_sbox();
        let mut inv_sbox = [0u8; 256];
        for (index, value) in sbox.iter().enumerate() {
            inv_sbox[*value as usize] = index as u8;
        }
        Ok(Self {
            sbox,
            inv_sbox,
            round_keys: expand_key(key, &sbox),
        })
    }

    fn add_round_key(&self, state: &mut [u8; BLOCK], round: usize) {
        for index in 0..BLOCK {
            state[index] ^= self.round_keys[round][index];
        }
    }

    /// Encrypts one block.
    pub fn encrypt_block(&self, input: &[u8; BLOCK], output: &mut [u8; BLOCK]) {
        let mut state = *input;
        self.add_round_key(&mut state, 0);
        for round in 1..ROUNDS {
            sub_bytes(&self.sbox, &mut state);
            shift_rows(&mut state);
            mix_columns(&mut state);
            self.add_round_key(&mut state, round);
        }
        sub_bytes(&self.sbox, &mut state);
        shift_rows(&mut state);
        self.add_round_key(&mut state, ROUNDS);
        *output = state;
    }

    /// Decrypts one block.
    pub fn decrypt_block(&self, input: &[u8; BLOCK], output: &mut [u8; BLOCK]) {
        let mut state = *input;
        self.add_round_key(&mut state, ROUNDS);
        for round in (1..ROUNDS).rev() {
            inv_shift_rows(&mut state);
            for byte in state.iter_mut() {
                *byte = self.inv_sbox[*byte as usize];
            }
            self.add_round_key(&mut state, round);
            inv_mix_columns(&mut state);
        }
        inv_shift_rows(&mut state);
        for byte in state.iter_mut() {
            *byte = self.inv_sbox[*byte as usize];
        }
        self.add_round_key(&mut state, 0);
        *output = state;
    }
}

/// The IV a segment uses when the playlist carries no explicit one: the
/// segment's sequence number as a 128-bit big-endian value.
pub fn iv_from_sequence(sequence: u64) -> [u8; BLOCK] {
    let mut iv = [0u8; BLOCK];
    iv[8..].copy_from_slice(&sequence.to_be_bytes());
    iv
}

/// Appends PKCS#7 padding so the buffer is a whole number of blocks. A
/// buffer that already is a multiple gets a full extra block of padding,
/// which is the standard's rule and keeps decryption unambiguous.
pub fn pkcs7_pad(data: &mut Vec<u8>) {
    let pad = BLOCK - (data.len() % BLOCK);
    data.resize(data.len() + pad, pad as u8);
}

/// Strips PKCS#7 padding and reports the original length.
pub fn pkcs7_unpad(data: &[u8]) -> Result<usize, AesError> {
    let length = data.len();
    if length == 0 {
        return Err(AesError::BadPadding);
    }
    let pad = data[length - 1] as usize;
    if pad == 0 || pad > BLOCK || pad > length {
        return Err(AesError::BadPadding);
    }
    if !data[length - pad..]
        .iter()
        .all(|byte| *byte as usize == pad)
    {
        return Err(AesError::BadPadding);
    }
    Ok(length - pad)
}

/// Encrypts `data` in place. Padding is appended first, so the caller's
/// buffer grows and ends up holding the whole ciphertext.
pub fn encrypt_pkcs7(cipher: &Aes128, iv: &[u8; BLOCK], data: &mut Vec<u8>) {
    pkcs7_pad(data);
    let mut previous = *iv;
    let mut block = [0u8; BLOCK];
    for start in (0..data.len()).step_by(BLOCK) {
        let end = start + BLOCK;
        for index in start..end {
            data[index] ^= previous[index - start];
        }
        cipher.encrypt_block(
            data[start..end]
                .try_into()
                .expect("padding guarantees whole blocks"),
            &mut block,
        );
        data[start..end].copy_from_slice(&block);
        previous = block;
    }
}

/// Decrypts `data` in place and truncates it to the unpadded length.
pub fn decrypt_pkcs7(
    cipher: &Aes128,
    iv: &[u8; BLOCK],
    data: &mut Vec<u8>,
) -> Result<usize, AesError> {
    if data.is_empty() || data.len() % BLOCK != 0 {
        return Err(AesError::BadLength);
    }
    let mut previous = *iv;
    let mut block = [0u8; BLOCK];
    for start in (0..data.len()).step_by(BLOCK) {
        let end = start + BLOCK;
        let mut current = [0u8; BLOCK];
        current.copy_from_slice(&data[start..end]);
        cipher.decrypt_block(&current, &mut block);
        for index in 0..BLOCK {
            block[index] ^= previous[index];
        }
        data[start..end].copy_from_slice(&block);
        previous = current;
    }
    let plain = pkcs7_unpad(data)?;
    data.truncate(plain);
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex pair"))
            .collect()
    }

    fn arr(bytes: &[u8]) -> [u8; BLOCK] {
        let mut out = [0u8; BLOCK];
        out.copy_from_slice(bytes);
        out
    }

    fn hex_string(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn the_sbox_matches_the_standard_first_row() {
        let sbox = build_sbox();
        assert_eq!(hex_string(&sbox[0..16]), "637c777bf26b6fc53001672bfed7ab76");
        assert_eq!(sbox[0x00], 0x63);
        assert_eq!(sbox[0x53], 0xed);
        assert_eq!(sbox[0xff], 0x16);
    }

    #[test]
    fn the_fips_197_single_block_vector() {
        let key = hex("000102030405060708090a0b0c0d0e0f");
        let plain = hex("00112233445566778899aabbccddeeff");
        let expected = hex("69c4e0d86a7b0430d8cdb78070b4c55a");
        let cipher = Aes128::new(&key).expect("key");
        let mut out = [0u8; BLOCK];
        cipher.encrypt_block(&arr(&plain), &mut out);
        assert_eq!(hex_string(&out), hex_string(&expected));

        let mut back = [0u8; BLOCK];
        cipher.decrypt_block(&out, &mut back);
        assert_eq!(&back[..], &plain[..]);
    }

    #[test]
    fn the_nist_sp800_38a_cbc_vector() {
        let key = hex("2b7e151628aed2a6abf7158809cf4f3c");
        let iv = hex("000102030405060708090a0b0c0d0e0f");
        let plain = hex("6bc1bee22e409f96e93d7e117393172a");
        let expected = hex("7649abac8119b246cee98e9b12e9197d");
        let cipher = Aes128::new(&key).expect("key");

        let mut data = plain.clone();
        encrypt_pkcs7(&cipher, &arr(&iv), &mut data);
        assert_eq!(hex_string(&data[..BLOCK]), hex_string(&expected));

        let mut plain_back = data;
        let original = decrypt_pkcs7(&cipher, &arr(&iv), &mut plain_back).expect("decrypt");
        assert_eq!(original, 16);
        assert_eq!(&plain_back[..16], &plain[..]);
    }

    #[test]
    fn every_block_round_trips_and_changes_with_the_iv() {
        let key = hex("0f0e0d0c0b0a09080706050403020100");
        let cipher = Aes128::new(&key).expect("key");
        let mut buffer = Vec::new();
        for index in 0..200u32 {
            buffer.push((index * 37 % 251) as u8);
        }

        let first_iv = iv_from_sequence(1);
        let second_iv = iv_from_sequence(2);
        let mut encrypted_first = buffer.clone();
        encrypt_pkcs7(&cipher, &first_iv, &mut encrypted_first);
        let mut encrypted_second = buffer.clone();
        encrypt_pkcs7(&cipher, &second_iv, &mut encrypted_second);

        assert_ne!(encrypted_first, encrypted_second, "the IV must matter");
        assert_eq!(encrypted_first.len() % BLOCK, 0);

        let mut restored = encrypted_first;
        let original = decrypt_pkcs7(&cipher, &first_iv, &mut restored).expect("decrypt");
        assert_eq!(original, buffer.len());
        assert_eq!(restored, buffer);
    }

    #[test]
    fn pkcs7_pads_a_full_block_and_reports_it() {
        let mut data = vec![7u8; BLOCK];
        pkcs7_pad(&mut data);
        assert_eq!(data.len(), 2 * BLOCK);
        assert_eq!(&data[BLOCK..], &[16u8; BLOCK]);
        assert_eq!(pkcs7_unpad(&data).expect("padding"), BLOCK);
    }

    #[test]
    fn pkcs7_rejects_a_tail_that_is_not_self_consistent() {
        let mut data = vec![0u8; BLOCK];
        data[BLOCK - 1] = 5;
        assert_eq!(pkcs7_unpad(&data), Err(AesError::BadPadding));
    }

    #[test]
    fn a_wrong_key_length_is_refused() {
        assert!(matches!(
            Aes128::new(&[0u8; 8]),
            Err(AesError::BadKeyLength)
        ));
        assert!(matches!(
            Aes128::new(&[0u8; 24]),
            Err(AesError::BadKeyLength)
        ));
    }
}
