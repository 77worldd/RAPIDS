use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::EngineError;

const HEX_LOWER: [u8; 16] = *b"0123456789abcdef";

/// Lowercase hex SHA-256 of an in-memory buffer.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    encode(digest.as_slice())
}

/// Lowercase hex SHA-256 of a file, streamed in small chunks.
pub fn sha256_file(path: &Path) -> Result<String, EngineError> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| EngineError::io("open file for hashing", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| EngineError::io("read file for hashing", path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    Ok(encode(digest.as_slice()))
}

fn encode(digest: &[u8]) -> String {
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(HEX_LOWER[(byte >> 4) as usize] as char);
        output.push(HEX_LOWER[(byte & 0x0f) as usize] as char);
    }
    output
}
