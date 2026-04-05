/// Test that verifies a CLI-generated signature in Rust
/// This test checks that the signature format from the CLI's pqc-sign binary
/// is compatible with the node's verification logic.

use pqcrypto_dilithium::dilithium5;
use pqcrypto_traits::sign::{PublicKey, SecretKey, SignedMessage};

#[test]
fn test_cli_signature_format() {
    // Generate a keypair
    let (pk, sk) = dilithium5::keypair();
    
    // Create a test message (hash)
    let msg = b"test message hash";
    
    // Sign the message (this is what CLI does)
    let signed_msg = dilithium5::sign(msg, &sk);
    
    // Print sizes for debugging
    println!("Message length: {}", msg.len());
    println!("SignedMessage length: {}", signed_msg.as_bytes().len());
    println!("Expected signature bytes: {}", dilithium5::signature_bytes());
    println!("Expected SignedMessage = msg.len() + signature_bytes = {}", msg.len() + dilithium5::signature_bytes());
    
    // The SignedMessage contains: signature || message
    // Let's verify this
    let signed_bytes = signed_msg.as_bytes();
    
    // According to pqcrypto docs, SignedMessage format is: [signature_bytes][message_bytes]
    let sig_len = dilithium5::signature_bytes();
    assert!(signed_bytes.len() >= sig_len, "SignedMessage too short");
    
    let (sig_part, msg_part) = signed_bytes.split_at(sig_len);
    println!("Signature part length: {}", sig_part.len());
    println!("Message part length: {}", msg_part.len());
    println!("Message part matches original: {}", msg_part == msg);
    
    // Now verify using dilithium5::open (this is what the node does)
    let opened = dilithium5::open(&signed_msg, &pk).expect("Failed to verify signature");
    assert_eq!(opened, msg, "Opened message doesn't match original");
    
    // Now let's try what the node does: pass the full SignedMessage as "sig"
    // This should work because verify_dilithium5 calls SignedMessage::from_bytes
    let reconstructed_sm = dilithium5::SignedMessage::from_bytes(signed_bytes)
        .expect("Failed to reconstruct SignedMessage");
    let opened2 = dilithium5::open(&reconstructed_sm, &pk)
        .expect("Failed to verify reconstructed signature");
    assert_eq!(opened2, msg, "Opened message from reconstructed doesn't match");
    
    println!("\n✓ CLI signature format is compatible with node verification");
}

#[test]
fn test_node_verification_with_cli_format() {
    // This test simulates what happens in the node
    use sha3::{Digest, Sha3_256};
    
    // Generate keypair
    let (pk, sk) = dilithium5::keypair();
    
    // Create a transaction-like JSON and hash it
    let json = r#"{"from":"addr1","to":"addr2","amount":100,"nonce":1}"#;
    let mut hasher = Sha3_256::new();
    hasher.update(json.as_bytes());
    let hash = hasher.finalize();
    
    println!("Transaction JSON: {}", json);
    println!("Hash (hex): {}", hex::encode(&hash));
    println!("Hash length: {}", hash.len());
    
    // CLI signs the hash
    let signed_msg = dilithium5::sign(&hash, &sk);
    let cli_signature_hex = hex::encode(signed_msg.as_bytes());
    
    println!("CLI signature length: {}", signed_msg.as_bytes().len());
    println!("CLI signature (first 64 hex chars): {}", &cli_signature_hex[..64.min(cli_signature_hex.len())]);
    
    // Node receives the hex signature and converts it back
    let sig_bytes = hex::decode(&cli_signature_hex).expect("Failed to decode hex");
    
    // Node tries to verify (this is what verify_dilithium5 does)
    let signed_msg_node = dilithium5::SignedMessage::from_bytes(&sig_bytes)
        .expect("Failed to parse SignedMessage from CLI signature");
    
    let opened_msg = dilithium5::open(&signed_msg_node, &pk)
        .expect("Node failed to verify CLI signature");
    
    println!("Opened message length: {}", opened_msg.len());
    println!("Opened message matches hash: {}", opened_msg.as_slice() == hash.as_slice());
    
    assert_eq!(opened_msg.as_slice(), hash.as_slice(), "Verification failed: opened message doesn't match hash");
    
    println!("\n✓ Node can successfully verify CLI-generated signatures");
}

#[test]
fn test_signature_components() {
    // This test explores the internal structure of SignedMessage
    let (pk, sk) = dilithium5::keypair();
    let msg = b"hello world";
    
    println!("\n=== Dilithium5 Format Analysis ===");
    println!("Public key bytes: {}", dilithium5::public_key_bytes());
    println!("Secret key bytes: {}", dilithium5::secret_key_bytes());
    println!("Signature bytes: {}", dilithium5::signature_bytes());
    
    let signed_msg = dilithium5::sign(msg, &sk);
    let signed_bytes = signed_msg.as_bytes();
    
    println!("\nMessage: {:?}", std::str::from_utf8(msg).unwrap());
    println!("Message length: {}", msg.len());
    println!("SignedMessage total length: {}", signed_bytes.len());
    println!("Expected: {} (sig) + {} (msg) = {}", 
        dilithium5::signature_bytes(), 
        msg.len(), 
        dilithium5::signature_bytes() + msg.len()
    );
    
    // Verify the structure
    let sig_len = dilithium5::signature_bytes();
    let (sig_part, msg_part) = signed_bytes.split_at(sig_len);
    
    println!("\nSignedMessage structure:");
    println!("  First {} bytes (signature): {}", sig_len, hex::encode(&sig_part[..32.min(sig_len)]));
    println!("  Remaining {} bytes (message): {:?}", msg_part.len(), std::str::from_utf8(msg_part).ok());
    
    assert_eq!(msg_part, msg, "Message part doesn't match original");
    assert_eq!(signed_bytes.len(), sig_len + msg.len(), "SignedMessage length mismatch");
    
    // Verify
    let opened = dilithium5::open(&signed_msg, &pk).expect("Verification failed");
    assert_eq!(opened, msg);
    
    println!("\n✓ SignedMessage format confirmed: [signature][message]");
}
