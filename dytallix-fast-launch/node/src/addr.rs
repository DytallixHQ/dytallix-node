// Address derivation module for Node - mirrors CLI implementation for compatibility

use blake3;
use sha2::{Digest, Sha256};

pub fn get_address(pubkey: &[u8]) -> String {
    // Step 1: Hash the public key using Blake3 (32 bytes output)
    let hash = blake3::hash(pubkey);
    let hash_bytes = hash.as_bytes();

    // Step 2: Take first 20 bytes of the hash (address portion)
    let address_bytes = &hash_bytes[..20];

    // Step 3: Calculate checksum using SHA256 of the 20-byte hash
    let mut sha256 = Sha256::new();
    sha256.update(address_bytes);
    let checksum = sha256.finalize();

    // Step 4: Append first 4 bytes of checksum (error detection)
    let checksum_bytes = &checksum[..4];

    // Step 5: Combine address and checksum (24 bytes total)
    let mut full_bytes = [0u8; 24];
    full_bytes[..20].copy_from_slice(address_bytes);
    full_bytes[20..].copy_from_slice(checksum_bytes);

    // Step 6: Encode in hexadecimal and add prefix "dyt1"
    format!("dyt1{}", hex::encode(full_bytes))
}

pub fn validate_address(address: &str) -> bool {
    // Check prefix
    if !address.starts_with("dyt1") {
        return false;
    }

    // Check total length (4 prefix + 48 hex chars = 52 total)
    if address.len() != 52 {
        return false;
    }

    // Extract hex part and decode
    let hex_part = &address[4..];
    let bytes = match hex::decode(hex_part) {
        Ok(b) => b,
        Err(_) => return false,
    };

    // Should be exactly 24 bytes
    if bytes.len() != 24 {
        return false;
    }

    // Split into address (20 bytes) and checksum (4 bytes)
    let address_bytes = &bytes[..20];
    let provided_checksum = &bytes[20..];

    // Recalculate checksum
    let mut sha256 = Sha256::new();
    sha256.update(address_bytes);
    let calculated_checksum = sha256.finalize();

    // Compare first 4 bytes of calculated checksum with provided
    &calculated_checksum[..4] == provided_checksum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_generation_deterministic() {
        let pubkey = b"test_public_key_123456789012345678901234567890";
        let addr1 = get_address(pubkey);
        let addr2 = get_address(pubkey);
        assert_eq!(addr1, addr2);
    }

    #[test]
    fn test_address_format() {
        let pubkey = b"test_public_key_123456789012345678901234567890";
        let address = get_address(pubkey);

        // Should start with "dyt1"
        assert!(address.starts_with("dyt1"));

        // Should be exactly 52 characters
        assert_eq!(address.len(), 52);

        // Should be valid hex after prefix
        let hex_part = &address[4..];
        assert!(hex::decode(hex_part).is_ok());
    }

    #[test]
    fn test_address_validation() {
        let pubkey = b"test_public_key_123456789012345678901234567890";
        let valid_address = get_address(pubkey);

        // Valid address should pass validation
        assert!(validate_address(&valid_address));

        // Invalid prefix
        assert!(!validate_address(
            "btc1e1c820e653bb12629306be2af671e2aab83074cdf6193cf6"
        ));

        // Wrong length
        assert!(!validate_address(
            "dyt1e1c820e653bb12629306be2af671e2aab83074cdf6193c"
        ));

        // Invalid hex
        assert!(!validate_address(
            "dyt1g1c820e653bb12629306be2af671e2aab83074cdf6193cf6"
        ));

        // Corrupted checksum
        let mut corrupted = valid_address.clone();
        corrupted.pop();
        corrupted.push('0');
        assert!(!validate_address(&corrupted));
    }

    #[test]
    fn test_different_pubkeys_different_addresses() {
        let pubkey1 = b"test_public_key_1";
        let pubkey2 = b"test_public_key_2";

        let addr1 = get_address(pubkey1);
        let addr2 = get_address(pubkey2);

        assert_ne!(addr1, addr2);
    }

    #[test]
    fn test_validate_corrupted_address() {
        let pubkey = b"test_public_key_123456789012345678901234567890";
        let valid_address = get_address(pubkey);

        // Test corrupting a single character
        let mut chars: Vec<char> = valid_address.chars().collect();
        chars[10] = if chars[10] == 'a' { 'b' } else { 'a' }; // Change one character
        let corrupted_address: String = chars.into_iter().collect();

        assert!(!validate_address(&corrupted_address));
    }
}
