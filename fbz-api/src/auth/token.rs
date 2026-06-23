use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};

const TOKEN_BYTES: usize = 32;
const TOKEN_PREFIX_CHARS: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuedToken {
    pub token: String,
    pub hash: Vec<u8>,
    pub prefix: String,
}

pub fn issue_access_token() -> IssuedToken {
    let mut bytes = [0_u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    let token = hex_encode(&bytes);
    let hash = hash_token(&token);
    let prefix = token.chars().take(TOKEN_PREFIX_CHARS).collect();

    IssuedToken {
        token,
        hash,
        prefix,
    }
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_is_random_hashable_and_prefixed() {
        let first = issue_access_token();
        let second = issue_access_token();

        assert_ne!(first.token, second.token);
        assert_eq!(first.token.len(), TOKEN_BYTES * 2);
        assert_eq!(first.hash, hash_token(&first.token));
        assert_eq!(first.prefix.len(), TOKEN_PREFIX_CHARS);
    }
}
