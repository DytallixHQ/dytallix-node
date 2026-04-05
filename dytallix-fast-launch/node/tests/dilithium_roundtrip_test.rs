/// Integration test to verify end-to-end Dilithium5 signing and verification
#[cfg(feature = "pqc-real")]
#[test]
fn test_dilithium5_sign_and_verify_roundtrip() {
    use pqcrypto_dilithium::dilithium5;
    use pqcrypto_traits::sign::{PublicKey, SecretKey, SignedMessage};
    
    // Generate a keypair
    let (pk, sk) = dilithium5::keypair();
    
    // Message to sign (simulating a transaction hash)
    let message = b"test message to sign";
    
    // Sign the message
    let signed_msg = dilithium5::sign(message, &sk);
    
    // Verify the signature
    let opened = dilithium5::open(&signed_msg, &pk).expect("Verification should succeed");
    
    assert_eq!(opened.as_slice(), message, "Opened message should match original");
    
    println!("✅ Dilithium5 roundtrip test passed");
    println!("PK len: {}", pk.as_bytes().len());
    println!("SK len: {}", sk.as_bytes().len());
    println!("Signed message len: {}", signed_msg.as_bytes().len());
}
