//! Minimal PQC evidence helper
//! Feature gated behind `pqc-real` per orchestrator acceptance.
//! Generates keypair, signs canonical tx bytes, verifies OK, then tamper verifies FAIL.
//! Outputs artifacts to a provided directory argument (default: ./pqc_artifacts).

use base64::Engine;
use dytallix_pqc::PQCManager;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf; // bring encode into scope

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "pqc-real"))]
    {
        eprintln!("pqc_evidence built without 'pqc-real' feature; no-op");
        return Ok(());
    }

    #[cfg(feature = "pqc-real")]
    {
        use sha3::{Digest, Sha3_256};
        let out_dir = std::env::args()
            .nth(1)
            .unwrap_or_else(|| "pqc_artifacts".to_string());
        let out = PathBuf::from(out_dir);
        fs::create_dir_all(&out)?;

        // 1) Keypair (Dilithium5 default via PQCManager)
        let manager = PQCManager::new()?; // Dilithium5
        let pubkey = manager.get_signature_public_key();
        let algo = manager.get_signature_algorithm();
        let pub_hex = hex::encode(pubkey);
        let mut f = File::create(out.join("pubkey.hex"))?;
        f.write_all(pub_hex.as_bytes())?;

        // Canonical tx bytes (placeholder deterministic example)
        let canonical = br#"{\"chain_id\":\"dytallix-testnet\",\"from\":\"demo1\",\"to\":\"demo2\",\"amount\":12345,\"nonce\":1}"#;

        // Hash canonical bytes for signing (explicit)
        let mut hasher = Sha3_256::new();
        hasher.update(canonical);
        let digest = hasher.finalize();

        let sig = manager.sign(&digest)?;
        let signed = serde_json::json!({
            "algorithm": format!("{:?}", algo),
            "pubkey_hex": pub_hex,
            "payload_b64": b64(&digest),
            "sig_b64": b64(&sig.data),
        });
        fs::write(
            out.join("signed_tx.json"),
            serde_json::to_string_pretty(&signed)?,
        )?;

        // Verify OK
        let ok = manager.verify(&digest, &sig, pubkey)?;
        let mut ok_log = File::create(out.join("verify_ok.log"))?;
        if ok {
            writeln!(ok_log, "VERIFY_OK")?;
        } else {
            writeln!(ok_log, "VERIFY_FAIL")?;
        }

        // Tamper (flip one bit in digest) then verify should FAIL
        let mut tampered = digest.to_vec();
        if let Some(b) = tampered.get_mut(0) {
            *b ^= 0x01;
        }
        let fail = manager.verify(&tampered, &sig, pubkey)?;
        let mut fail_log = File::create(out.join("verify_fail_tamper.log"))?;
        if fail {
            writeln!(fail_log, "VERIFY_OK")?;
        } else {
            writeln!(fail_log, "VERIFY_FAIL")?;
        }

        // Emit sizes summary for orchestrator consumption (stdout JSON)
        let summary = serde_json::json!({
            "algorithm": format!("{:?}", algo),
            "pubkey_bytes": pubkey.len(),
            "signature_bytes": sig.data.len(),
            "verify_ok": ok,
            "verify_fail_expected": !fail,
        });
        println!("{}", serde_json::to_string(&summary)?);
    }
    Ok(())
}
