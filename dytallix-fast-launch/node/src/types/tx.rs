use crate::crypto::{canonical_json, sha3_256, verify, ActivePQC, PQCAlgorithm, PQC};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

// Helper module to (de)serialize u128 as string for canonical on-wire format.
mod as_str_u128 {
    use serde::{self, Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &u128, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u128, D::Error> {
        let s: String = Deserialize::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {
    Send {
        from: String,
        to: String,
        denom: String,
        #[serde(with = "as_str_u128")]
        amount: u128,
    },
    Data {
        from: String,
        data: String, // JSON or arbitrary string data to anchor on-chain
    },
    DmsRegister {
        from: String,
        beneficiary: String,
        #[serde(with = "as_str_u128")]
        period: u128,
    },
    DmsPing {
        from: String,
    },
    DmsClaim {
        from: String,
        owner: String,
    },
}

impl Msg {
    pub fn validate(&self) -> Result<()> {
        match self {
            Msg::Send {
                from,
                to,
                denom,
                amount,
            } => {
                eprintln!("[DEBUG msg validate] amount={}, denom='{}', from='{}', to='{}'", amount, denom, from, to);
                if *amount == 0 {
                    return Err(anyhow!("amount cannot be zero"));
                }
                if from.is_empty() {
                    return Err(anyhow!("from address cannot be empty"));
                }
                if to.is_empty() {
                    return Err(anyhow!("to address cannot be empty"));
                }
                // Accept both micro-denominations (udgt, udrt) and whole tokens (DGT, DRT)
                let up = denom.to_ascii_uppercase();
                eprintln!("[DEBUG msg validate] denom uppercase: '{}', checking if DGT, DRT, UDGT, or UDRT", up);
                if up != "DGT" && up != "DRT" && up != "UDGT" && up != "UDRT" {
                    eprintln!("[DEBUG msg validate] ❌ DENOM VALIDATION FAILED: got '{}', expected DGT, DRT, udgt, or udrt", denom);
                    return Err(anyhow!("unsupported denom: {}; valid: DGT, DRT, udgt, udrt", denom));
                }
                eprintln!("[DEBUG msg validate] ✅ Message validation passed");
            }
            Msg::Data { from, data } => {
                if from.is_empty() {
                    return Err(anyhow!("from address cannot be empty"));
                }
                if data.is_empty() {
                    return Err(anyhow!("data cannot be empty"));
                }
                // Limit data size to 1MB
                if data.len() > 1_000_000 {
                    return Err(anyhow!("data too large: max 1MB"));
                }
            }
            Msg::DmsRegister { from, beneficiary, period } => {
                if from.is_empty() {
                    return Err(anyhow!("from address cannot be empty"));
                }
                if beneficiary.is_empty() {
                    return Err(anyhow!("beneficiary address cannot be empty"));
                }
                if *period == 0 {
                    return Err(anyhow!("period cannot be zero"));
                }
            }
            Msg::DmsPing { from } => {
                if from.is_empty() {
                    return Err(anyhow!("from address cannot be empty"));
                }
            }
            Msg::DmsClaim { from, owner } => {
                if from.is_empty() {
                    return Err(anyhow!("from address cannot be empty"));
                }
                if owner.is_empty() {
                    return Err(anyhow!("owner address cannot be empty"));
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn sender(&self) -> &str {
        match self {
            Msg::Send { from, .. } => from,
            Msg::Data { from, .. } => from,
            Msg::DmsRegister { from, .. } => from,
            Msg::DmsPing { from, .. } => from,
            Msg::DmsClaim { from, .. } => from,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tx {
    pub chain_id: String,
    pub nonce: u64,
    pub msgs: Vec<Msg>,
    #[serde(with = "as_str_u128")]
    pub fee: u128,
    pub memo: String,
}

impl Tx {
    pub fn new(
        chain_id: impl Into<String>,
        nonce: u64,
        msgs: Vec<Msg>,
        fee: u128,
        memo: impl Into<String>,
    ) -> Result<Self> {
        if msgs.is_empty() {
            return Err(anyhow!("transaction must contain at least one message"));
        }
        for m in &msgs {
            m.validate()?;
        }
        if fee == 0 {
            return Err(anyhow!("fee cannot be zero"));
        }
        let chain_id = chain_id.into();
        if chain_id.is_empty() {
            return Err(anyhow!("chain_id cannot be empty"));
        }
        Ok(Self {
            chain_id,
            nonce,
            msgs,
            fee,
            memo: memo.into(),
        })
    }

    pub fn validate(&self, expected_chain_id: &str) -> Result<()> {
        eprintln!("[DEBUG validate] Comparing chain IDs:");
        eprintln!("[DEBUG validate]   Expected: '{}' (len: {})", expected_chain_id, expected_chain_id.len());
        eprintln!("[DEBUG validate]   Got:      '{}' (len: {})", self.chain_id, self.chain_id.len());
        eprintln!("[DEBUG validate]   Match: {}", self.chain_id == expected_chain_id);
        
        if self.chain_id != expected_chain_id {
            return Err(anyhow!(
                "invalid chain_id: expected {}, got {}",
                expected_chain_id,
                self.chain_id
            ));
        }
        if self.msgs.is_empty() {
            return Err(anyhow!("transaction must contain at least one message"));
        }
        if self.fee == 0 {
            return Err(anyhow!("fee cannot be zero"));
        }
        for msg in &self.msgs {
            msg.validate()?;
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<[u8; 32]> {
        let bytes = canonical_json(self)?;
        Ok(sha3_256(&bytes))
    }

    pub fn tx_hash(&self) -> Result<String> {
        let hash = self.canonical_hash()?;
        Ok(format!("0x{}", hex::encode(hash)))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SignedTx {
    pub tx: Tx,
    pub public_key: String, // base64
    pub signature: String,  // base64
    pub algorithm: String,  // ActivePQC::ALG
    pub version: u32,       // 1
}

impl SignedTx {
    pub fn sign(tx: Tx, sk: &[u8], pk: &[u8]) -> Result<Self> {
        let bytes = canonical_json(&tx)?;
        let hash = sha3_256(&bytes);
        let sig = ActivePQC::sign(sk, &hash);
        Ok(Self {
            tx,
            public_key: B64.encode(pk),
            signature: B64.encode(sig),
            algorithm: ActivePQC::ALG.to_string(),
            version: 1,
        })
    }

    pub fn verify(&self) -> Result<()> {
        // Parse the algorithm from the string
        let algorithm = PQCAlgorithm::from_str(&self.algorithm)
            .map_err(|e| anyhow!("invalid algorithm '{}': {}", self.algorithm, e))?;

        if self.version != 1 {
            return Err(anyhow!(
                "unsupported version: expected 1, got {}",
                self.version
            ));
        }
        let bytes = canonical_json(&self.tx)?;
        let hash = sha3_256(&bytes);
        
        eprintln!("[DEBUG verify] Canonical JSON: {}", String::from_utf8_lossy(&bytes));
        eprintln!("[DEBUG verify] Hash: {}", hex::encode(&hash));
        eprintln!("[DEBUG verify] Signature (base64 len): {}", self.signature.len());
        eprintln!("[DEBUG verify] Public key (base64 len): {}", self.public_key.len());
        
        let sig = B64
            .decode(&self.signature)
            .map_err(|e| anyhow!("invalid signature encoding: {}", e))?;
        let pk = B64
            .decode(&self.public_key)
            .map_err(|e| anyhow!("invalid public key encoding: {}", e))?;

        eprintln!("[DEBUG verify] Signature (decoded len): {}", sig.len());
        eprintln!("[DEBUG verify] Public key (decoded len): {}", pk.len());
        eprintln!("[DEBUG verify] Algorithm: {:?}", algorithm);

        // Use the new multi-algorithm verification
        match verify(&pk, &hash, &sig, algorithm) {
            Ok(()) => {
                eprintln!("[DEBUG verify] ✅ SIGNATURE VERIFICATION PASSED!");
                Ok(())
            }
            Err(e) => {
                eprintln!("[DEBUG verify] ❌ SIGNATURE VERIFICATION FAILED: {}", e);
                Err(anyhow!("signature verification failed: {}", e))
            }
        }
    }

    pub fn tx_hash(&self) -> Result<String> {
        self.tx.tx_hash()
    }

    pub fn first_from_address(&self) -> Option<&str> {
        self.tx.msgs.first().map(|m| m.sender())
    }
}

// Validation helpers
#[derive(Debug, Clone, serde::Serialize)]
pub enum ValidationError {
    InvalidChainId { expected: String, got: String },
    InvalidNonce { expected: u64, got: u64 },
    InvalidSignature,
    InsufficientFunds {
        denom: String,
        required: u128,
        available: u128,
    },
    DuplicateTransaction,
    MempoolFull,
    Internal(String),
}

impl ValidationError {
    pub fn http_status(&self) -> u16 {
        match self {
            ValidationError::InvalidChainId { .. }
            | ValidationError::InvalidSignature
            | ValidationError::Internal(_) => 422, // Unprocessable Entity
            ValidationError::InvalidNonce { .. } | ValidationError::DuplicateTransaction => 409, // Conflict
            ValidationError::InsufficientFunds { .. } => 422,
            ValidationError::MempoolFull => 503, // Service Unavailable
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            ValidationError::InvalidChainId { .. } => "INVALID_CHAIN_ID",
            ValidationError::InvalidNonce { .. } => "INVALID_NONCE",
            ValidationError::InvalidSignature => "INVALID_SIGNATURE",
            ValidationError::InsufficientFunds { .. } => "INSUFFICIENT_FUNDS",
            ValidationError::DuplicateTransaction => "DUPLICATE_TRANSACTION",
            ValidationError::MempoolFull => "MEMPOOL_FULL",
            ValidationError::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            ValidationError::InvalidChainId { expected, got } => {
                serde_json::json!({
                    "error": self.error_code(),
                    "message": format!("Invalid chain_id: expected {}, got {}", expected, got),
                    "expected": expected,
                    "got": got
                })
            }
            ValidationError::InvalidNonce { expected, got } => {
                serde_json::json!({
                    "error": self.error_code(),
                    "message": format!("Invalid nonce: expected {}, got {}", expected, got),
                    "expected": expected,
                    "got": got
                })
            }
            ValidationError::InvalidSignature => {
                serde_json::json!({
                    "error": self.error_code(),
                    "message": "Invalid signature"
                })
            }
            ValidationError::InsufficientFunds {
                denom,
                required,
                available,
            } => {
                serde_json::json!({
                    "error": self.error_code(),
                    "message": format!(
                        "Insufficient funds for {}: required {}, available {}",
                        denom, required, available
                    ),
                    "denom": denom,
                    "required": required.to_string(),
                    "available": available.to_string()
                })
            }
            ValidationError::DuplicateTransaction => {
                serde_json::json!({
                    "error": self.error_code(),
                    "message": "Transaction already exists"
                })
            }
            ValidationError::MempoolFull => {
                serde_json::json!({
                    "error": self.error_code(),
                    "message": "Mempool is full"
                })
            }
            ValidationError::Internal(msg) => {
                serde_json::json!({
                    "error": self.error_code(),
                    "message": msg
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_http_status() {
        assert_eq!(
            ValidationError::InvalidChainId {
                expected: "test".into(),
                got: "wrong".into()
            }
            .http_status(),
            422
        );
        assert_eq!(
            ValidationError::InvalidNonce {
                expected: 1,
                got: 2
            }
            .http_status(),
            409
        );
        assert_eq!(ValidationError::InvalidSignature.http_status(), 422);
        assert_eq!(
            ValidationError::InsufficientFunds {
                denom: "udgt".into(),
                required: 100,
                available: 50
            }
            .http_status(),
            422
        );
        assert_eq!(ValidationError::DuplicateTransaction.http_status(), 409);
        assert_eq!(ValidationError::MempoolFull.http_status(), 503);
    }

    #[test]
    fn test_validation_error_json() {
        let err = ValidationError::InvalidNonce {
            expected: 5,
            got: 3,
        };
        let json = err.to_json();

        assert_eq!(json["error"], "INVALID_NONCE");
        assert_eq!(json["expected"], 5);
        assert_eq!(json["got"], 3);

        let insufficient = ValidationError::InsufficientFunds {
            denom: "udrt".into(),
            required: 1_000_000,
            available: 500_000,
        };
        let json = insufficient.to_json();
        assert_eq!(json["error"], "INSUFFICIENT_FUNDS");
        assert_eq!(json["denom"], "udrt");
        assert_eq!(json["required"], "1000000");
        assert_eq!(json["available"], "500000");
    }

    #[test]
    fn test_msg_validation() {
        let valid_msg = Msg::Send {
            from: "alice".into(),
            to: "bob".into(),
            denom: "DGT".into(),
            amount: 100,
        };
        assert!(valid_msg.validate().is_ok());

        let zero_amount = Msg::Send {
            from: "alice".into(),
            to: "bob".into(),
            denom: "DGT".into(),
            amount: 0,
        };
        assert!(zero_amount.validate().is_err());
    }

    #[test]
    fn test_serde_u128_string() {
        let msg = Msg::Send {
            from: "alice".into(),
            to: "bob".into(),
            denom: "DGT".into(),
            amount: 1000000000000000000u128,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"1000000000000000000\""));

        let parsed: Msg = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, msg);
    }
}
