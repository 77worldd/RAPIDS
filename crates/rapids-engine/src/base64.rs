//! Base64 encoding, used for HTTP basic credentials (RFC 7617).
//!
//! RAPIDS needs exactly one encoding, so it lives here rather than behind a
//! dependency. Input bytes are taken as they are: a user name with non-ASCII
//! characters produces the same bytes a browser would send.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64, padded with `=`.
pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = u32::from(*chunk.get(1).unwrap_or(&0));
        let third = u32::from(*chunk.get(2).unwrap_or(&0));
        let packed = (first << 16) | (second << 8) | third;
        out.push(ALPHABET[(packed >> 18) as usize & 63] as char);
        out.push(ALPHABET[(packed >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(packed >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[packed as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_standard_table() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "Zg==");
        assert_eq!(encode(b"fo"), "Zm8=");
        assert_eq!(encode(b"rapid"), "cmFwaWQ=");
        assert_eq!(encode(b"rapids"), "cmFwaWRz");
        assert_eq!(encode(b"\xfb\xff"), "+/8=");
    }

    #[test]
    fn every_length_round_trips_through_the_alphabet() {
        for length in 0..40usize {
            let bytes: Vec<u8> = (0..length)
                .map(|index| ((index * 7 + 3) % 251) as u8)
                .collect();
            let text = encode(&bytes);
            assert_eq!(
                text.len() % 4,
                0,
                "padded output is a whole number of groups"
            );
            assert!(
                text.bytes().all(|byte| byte.is_ascii_alphanumeric()
                    || byte == b'+'
                    || byte == b'/'
                    || byte == b'='),
                "only the base64 alphabet and padding may appear"
            );
        }
    }
}
