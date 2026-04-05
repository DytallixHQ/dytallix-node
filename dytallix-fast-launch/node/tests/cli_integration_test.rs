/// Integration test that uses the actual CLI pqc-sign binary
/// to generate signatures and verifies them in the node

use pqcrypto_dilithium::dilithium5;
use pqcrypto_traits::sign::{PublicKey, SecretKey, SignedMessage};
use sha3::{Digest, Sha3_256};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn find_pqc_sign_binary() -> Option<PathBuf> {
    // Try to find the sign binary in the workspace
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent()?.to_path_buf();
    
    let candidates = vec![
        workspace_root.join("pqc-crypto/target/release/sign"),
        workspace_root.join("pqc-crypto/target/debug/sign"),
        workspace_root.join("../pqc-crypto/target/release/sign"),
        workspace_root.join("../pqc-crypto/target/debug/sign"),
    ];
    
    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    
    None
}

fn build_pqc_sign() -> Result<PathBuf, String> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Failed to get workspace root")?
        .to_path_buf();
    
    let pqc_crypto_dir = workspace_root.join("pqc-crypto");
    if !pqc_crypto_dir.exists() {
        let alt_dir = workspace_root.parent()
            .ok_or("Failed to get parent")?
            .join("pqc-crypto");
        if !alt_dir.exists() {
            return Err("pqc-crypto directory not found".to_string());
        }
        return build_in_dir(&alt_dir);
    }
    
    build_in_dir(&pqc_crypto_dir)
}

fn build_in_dir(dir: &PathBuf) -> Result<PathBuf, String> {
    println!("Building sign binary in {:?}", dir);
    let output = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--bin")
        .arg("sign")
        .current_dir(dir)
        .output()
        .map_err(|e| format!("Failed to run cargo: {}", e))?;
    
    if !output.status.success() {
        return Err(format!(
            "Failed to build sign:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    let binary = dir.join("target/release/sign");
    if !binary.exists() {
        return Err("sign binary not found after build".to_string());
    }
    
    Ok(binary)
}

#[test]
fn test_cli_binary_signature_verification() {
    // Find or build the sign binary
    let binary = match find_pqc_sign_binary() {
        Some(b) => {
            println!("Found sign binary at: {:?}", b);
            b
        }
        None => {
            println!("sign binary not found, building...");
            match build_pqc_sign() {
                Ok(b) => b,
                Err(e) => {
                    println!("⚠️  Skipping test: {}", e);
                    return;
                }
            }
        }
    };
    
    // Generate a keypair using the Rust library
    let (pk, sk) = dilithium5::keypair();
    
    // Create a test message (simulating a transaction hash)
    let json = r#"{"chain_id":"dytallix-testnet","fee":1000,"memo":"","msgs":[{"amount":100,"recipient":"dyt1recipient","type":"transfer"}],"nonce":1,"sender":"dyt1sender"}"#;
    let mut hasher = Sha3_256::new();
    hasher.update(json.as_bytes());
    let hash = hasher.finalize();
    
    println!("\n=== CLI Binary Integration Test ===");
    println!("Transaction JSON: {}", json);
    println!("Hash (hex): {}", hex::encode(&hash));
    println!("Hash length: {} bytes", hash.len());
    
    // Create temp directory for files
    let tmp = TempDir::new().expect("Failed to create temp dir");
    let sk_path = tmp.path().join("sk.bin");
    let msg_path = tmp.path().join("msg.bin");
    
    // Write secret key and message
    fs::write(&sk_path, sk.as_bytes()).expect("Failed to write secret key");
    fs::write(&msg_path, &hash).expect("Failed to write message");
    
    println!("Secret key file: {:?}", sk_path);
    println!("Message file: {:?}", msg_path);
    
    // Call the CLI binary to sign
    let output = Command::new(&binary)
        .arg(&sk_path)
        .arg(&msg_path)
        .output()
        .expect("Failed to run sign binary");
    
    if !output.status.success() {
        panic!(
            "sign binary failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    
    let sig_hex = String::from_utf8_lossy(&output.stdout).trim().to_string();
    println!("CLI signature (hex, first 64 chars): {}", &sig_hex[..64.min(sig_hex.len())]);
    println!("CLI signature total length: {} hex chars = {} bytes", sig_hex.len(), sig_hex.len() / 2);
    
    // Decode the signature
    let sig_bytes = hex::decode(&sig_hex).expect("Failed to decode hex signature");
    println!("Signature bytes: {}", sig_bytes.len());
    println!("Expected SignedMessage length: {} (sig) + {} (msg) = {}", 
        dilithium5::signature_bytes(), 
        hash.len(), 
        dilithium5::signature_bytes() + hash.len()
    );
    
    // Verify using the node's approach
    let signed_msg = dilithium5::SignedMessage::from_bytes(&sig_bytes)
        .expect("Failed to parse SignedMessage from CLI output");
    
    let opened = dilithium5::open(&signed_msg, &pk)
        .expect("Failed to verify CLI signature");
    
    println!("Opened message length: {}", opened.len());
    println!("Opened message (hex): {}", hex::encode(&opened));
    println!("Expected hash (hex): {}", hex::encode(&hash));
    
    assert_eq!(
        opened.as_slice(),
        hash.as_slice(),
        "Verification failed: opened message doesn't match hash"
    );
    
    println!("\n✅ SUCCESS: CLI binary signature verified in Rust node");
}

#[test]
fn test_base64_roundtrip() {
    // Test that base64 encoding/decoding works correctly for signatures
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    
    let (pk, sk) = dilithium5::keypair();
    let msg = b"test message";
    
    // Sign
    let signed_msg = dilithium5::sign(msg, &sk);
    let sig_bytes = signed_msg.as_bytes();
    
    println!("\n=== Base64 Encoding Test ===");
    println!("Original signature bytes: {}", sig_bytes.len());
    
    // Encode to base64 (like CLI does when sending to node)
    let sig_b64 = B64.encode(sig_bytes);
    println!("Base64 signature length: {} chars", sig_b64.len());
    
    // Decode from base64 (like node does when receiving)
    let decoded = B64.decode(&sig_b64).expect("Failed to decode base64");
    println!("Decoded bytes: {}", decoded.len());
    
    assert_eq!(decoded, sig_bytes, "Base64 roundtrip failed");
    
    // Verify the decoded signature
    let signed_msg_decoded = dilithium5::SignedMessage::from_bytes(&decoded)
        .expect("Failed to parse decoded SignedMessage");
    let opened = dilithium5::open(&signed_msg_decoded, &pk)
        .expect("Failed to verify decoded signature");
    
    assert_eq!(opened, msg);
    
    println!("✅ Base64 encoding/decoding works correctly");
}

#[test]
fn test_signature_format_details() {
    // Detailed analysis of signature format
    let (pk, sk) = dilithium5::keypair();
    
    // Use a 32-byte hash (like SHA3-256)
    let hash: [u8; 32] = [0x42; 32];
    
    println!("\n=== Signature Format Details ===");
    println!("Input hash: {} bytes", hash.len());
    println!("Hash (hex): {}", hex::encode(&hash));
    
    // Sign the hash
    let signed_msg = dilithium5::sign(&hash, &sk);
    let sig_bytes = signed_msg.as_bytes();
    
    println!("\nDilithium5 constants:");
    println!("  Public key bytes: {}", dilithium5::public_key_bytes());
    println!("  Secret key bytes: {}", dilithium5::secret_key_bytes());
    println!("  Signature bytes: {}", dilithium5::signature_bytes());
    
    println!("\nSignedMessage structure:");
    println!("  Total length: {}", sig_bytes.len());
    println!("  Expected: {} + {} = {}", 
        dilithium5::signature_bytes(), 
        hash.len(), 
        dilithium5::signature_bytes() + hash.len()
    );
    
    let sig_len = dilithium5::signature_bytes();
    let (sig_part, msg_part) = sig_bytes.split_at(sig_len);
    
    println!("\nComponents:");
    println!("  Signature part: {} bytes", sig_part.len());
    println!("  Message part: {} bytes", msg_part.len());
    println!("  Message part (hex): {}", hex::encode(msg_part));
    println!("  Message matches input: {}", msg_part == hash);
    
    // Verify
    let opened = dilithium5::open(&signed_msg, &pk)
        .expect("Verification failed");
    
    println!("\nVerification:");
    println!("  Opened message: {} bytes", opened.len());
    println!("  Opened (hex): {}", hex::encode(&opened));
    println!("  Matches input: {}", opened.as_slice() == hash);
    
    assert_eq!(opened.as_slice(), hash);
    
    println!("\n✅ Signature format verified");
}
