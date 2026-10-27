//! X link codes, v1 format adapted to NEAR accounts.
//!
//! `GM` + 2-digit offset + 10 hex characters of `sha256(account_id)` starting
//! at that offset + 2 random characters, all upper case: e.g. `GM07A1B2C3D4E5Q9`.
//! Tying the code to the account stops someone else's post from linking it.

use alloc::string::String;
use sha2::{Digest, Sha256};

pub const CODE_LEN: usize = 16;
const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const HEX: &[u8] = b"0123456789ABCDEF";
/// sha256 is 64 hex characters, so offsets 0..=54 leave room for 10.
const MAX_OFFSET: u8 = 54;

fn account_hex(account_id: &str) -> [u8; 64] {
    let digest = Sha256::digest(account_id.as_bytes());
    let mut out = [0u8; 64];
    for (i, b) in digest.iter().enumerate() {
        out[i * 2] = HEX[(b >> 4) as usize];
        out[i * 2 + 1] = HEX[(b & 0xf) as usize];
    }
    out
}

/// Builds a code from 3 random bytes (offset, two random characters).
pub fn generate(account_id: &str, random: [u8; 3]) -> String {
    let offset = random[0] % (MAX_OFFSET + 1);
    let hex = account_hex(account_id);
    let mut code = String::with_capacity(CODE_LEN);
    code.push_str("GM");
    code.push((b'0' + offset / 10) as char);
    code.push((b'0' + offset % 10) as char);
    for &c in &hex[offset as usize..offset as usize + 10] {
        code.push(c as char);
    }
    code.push(ALPHABET[random[1] as usize % ALPHABET.len()] as char);
    code.push(ALPHABET[random[2] as usize % ALPHABET.len()] as char);
    code
}

/// Checks that `code` is well formed and belongs to `account_id`.
pub fn validate(code: &str, account_id: &str) -> bool {
    let b = code.as_bytes();
    if b.len() != CODE_LEN || &b[..2] != b"GM" {
        return false;
    }
    if !b[2].is_ascii_digit() || !b[3].is_ascii_digit() {
        return false;
    }
    let offset = (b[2] - b'0') * 10 + (b[3] - b'0');
    if offset > MAX_OFFSET {
        return false;
    }
    let hex = account_hex(account_id);
    b[4..14] == hex[offset as usize..offset as usize + 10]
        && b[14..].iter().all(|c| ALPHABET.contains(c))
}

/// True if the post text contains the code (case-insensitive, as X keeps case
/// but users may retype it).
pub fn post_contains(text: &str, code: &str) -> bool {
    text.to_ascii_uppercase()
        .contains(&code.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn generates_valid_codes() {
        let code = generate("alice.near", [7, 1, 35]);
        assert_eq!(code.len(), CODE_LEN);
        assert!(code.starts_with("GM07"));
        assert!(code.ends_with("1Z"));
        assert!(validate(&code, "alice.near"));
        assert!(!validate(&code, "bob.near"));
    }

    #[test]
    fn rejects_malformed_codes() {
        let code = generate("alice.near", [0, 0, 0]);
        assert!(!validate(&code[..15], "alice.near"));
        assert!(!validate(&code.replacen("GM", "XX", 1), "alice.near"));
        let mut bad = String::from(&code[..2]);
        bad.push_str("99");
        bad.push_str(&code[4..]);
        assert!(!validate(&bad, "alice.near"));
        assert!(!validate(&code.replace(&code[14..], "a!"), "alice.near"));
    }

    #[test]
    fn finds_code_in_post() {
        let code = generate("alice.near", [3, 4, 5]);
        let text = alloc::format!("Linking my GM account {} gm!", code.to_lowercase());
        assert!(post_contains(&text, &code));
        assert!(!post_contains("gm", &code));
    }

    proptest! {
        #[test]
        fn every_generated_code_validates(account in "[a-z0-9_.-]{2,64}", r in any::<[u8; 3]>()) {
            let code = generate(&account, r);
            prop_assert!(validate(&code, &account));
        }
    }
}
