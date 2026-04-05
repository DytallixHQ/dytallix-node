# Post-Quantum Cryptography (PQC) Implementation

This document describes the post-quantum cryptographic signature verification implementation in the Dytallix node.

## Overview

The node implements multi-algorithm PQC signature verification with support for:
- **Dilithium5** (default, always enabled)
- **Falcon1024** (feature-gated)
- **SPHINCS+ SHA2-128s-simple** (feature-gated)

## Feature Flags

### Available Features

| Feature | Algorithm | Status | Description |
|---------|-----------|--------|-------------|
| `pqc-real` | All | Default | Enables real PQC verification using pqcrypto crates |
| `pqc-mock` | All | Testing | Enables mock verification for testing/development |
| `falcon` | Falcon1024 | Optional | Enables Falcon1024 signature verification |
| `sphincs` | SPHINCS+ | Optional | Enables SPHINCS+ signature verification |

### Build Examples

```bash
# Default build (Dilithium5 only)
cargo build

# Build with Falcon support
cargo build --features falcon

# Build with SPHINCS+ support  
cargo build --features sphincs

# Build with all PQC algorithms
cargo build --features "falcon,sphincs"

# Build with mock verification (testing)
cargo build --no-default-features --features pqc-mock

# Full-featured build
cargo build --features "falcon,sphincs,full-node"
```

## Algorithm Support

### Dilithium5 (Default)
- **Status**: Always available when `pqc-real` feature is enabled
- **Security**: NIST Level 3
- **Key Size**: 1952 bytes (public), 4000 bytes (private)
- **Signature Size**: ~2420 bytes
- **Use Case**: Default algorithm for all transactions

### Falcon1024 (Optional)
- **Status**: Available with `falcon` feature flag
- **Security**: NIST Level 5
- **Key Size**: 1793 bytes (public), 2305 bytes (private)  
- **Signature Size**: Variable (~1280 bytes average)
- **Use Case**: High-security applications requiring smaller signatures

### SPHINCS+ SHA2-128s-simple (Optional)
- **Status**: Available with `sphincs` feature flag
- **Security**: NIST Level 1
- **Key Size**: 32 bytes (public), 64 bytes (private)
- **Signature Size**: ~7856 bytes
- **Use Case**: Applications requiring small keys, tolerating large signatures

## Integration Points

### 1. Transaction Verification (types/tx.rs)
```rust
// SignedTx verification now supports algorithm detection
impl SignedTx {
    pub fn verify(&self) -> Result<()> {
        let algorithm = PQCAlgorithm::from_str(&self.algorithm)?;
        verify(&pk, &hash, &sig, algorithm)?;
        Ok(())
    }
}
```

### 2. Mempool Admission (mempool/mod.rs)
```rust
// Mempool uses default algorithm (Dilithium5) for compatibility
fn verify_pqc_signature(tx: &Transaction, signature: &str, public_key: &str) -> Result<(), String> {
    match verify(&pk_bytes, &tx_hash, &sig_bytes, PQCAlgorithm::default()) {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("signature verification failed: {}", e)),
    }
}
```

### 3. Block Execution
- All transactions undergo signature verification before execution
- Invalid signatures cause transaction rejection with clear error messages
- Different algorithms are supported based on compile-time features

## Error Handling

The verification system provides structured errors:

```rust
pub enum PQCVerifyError {
    UnsupportedAlgorithm(String),           // Unknown algorithm
    InvalidPublicKey { algorithm, details }, // Malformed public key
    InvalidSignature { algorithm, details }, // Malformed signature
    VerificationFailed { algorithm },        // Signature verification failed
    FeatureNotCompiled { feature },         // Algorithm not compiled in
}
```

## Testing

### Test Vectors
Located in `launch-evidence/pqc/vectors.json`:
- Deterministic test cases for each algorithm
- Both valid and invalid signature scenarios
- Transaction-level test vectors

### Evidence Logs
- `verify_ok.log`: Successful verification examples
- `verify_fail_tamper.log`: Failed verification for tampered data

### Unit Tests
```bash
# Run PQC verification tests
cargo test pqc_verify

# Test with different feature combinations
cargo test --features falcon
cargo test --features sphincs
cargo test --features "falcon,sphincs"
```

## Security Considerations

1. **Algorithm Selection**: Dilithium5 is the default for balanced security/performance
2. **Feature Compilation**: Unused algorithms are not compiled in, reducing attack surface
3. **Input Validation**: All inputs are validated before cryptographic operations
4. **Error Handling**: Timing-safe error handling prevents side-channel attacks
5. **Memory Safety**: Uses pqcrypto crates with safety guarantees

## Performance

| Algorithm | Keygen | Sign | Verify | Key Size | Sig Size |
|-----------|--------|------|--------|----------|----------|
| Dilithium5| ~0.2ms | ~0.5ms | ~0.3ms | 2.5KB | 2.4KB |
| Falcon1024| ~1.5ms | ~2.0ms | ~0.1ms | 1.8KB | 1.3KB |
| SPHINCS+  | ~0.5ms | ~20ms  | ~0.8ms | 32B  | 7.9KB |

## Migration Guide

### From Single Algorithm (Old)
```rust
// Old way - hardcoded Dilithium5
if !ActivePQC::verify(&pk, &msg, &sig) {
    return Err("verification failed");
}
```

### To Multi Algorithm (New)
```rust
// New way - algorithm specified
let algorithm = PQCAlgorithm::from_str(&tx.algorithm)?;
verify(&pk, &msg, &sig, algorithm)?;
```

### Backward Compatibility
The `verify_default()` function maintains compatibility:
```rust
// Still works - uses Dilithium5
if !verify_default(&pk, &msg, &sig) {
    return Err("verification failed");
}
```

## Production Deployment

### Recommended Build
```bash
cargo build --release --features "falcon,metrics,oracle"
```

### Environment Variables
```bash
# Optional: Override algorithm selection
export DYTALLIX_PQC_ALGORITHM=dilithium5
export DYTALLIX_PQC_STRICT_VALIDATION=true
```

### Monitoring
- Monitor verification times and error rates
- Track algorithm usage distribution
- Alert on unusual verification failure patterns

## Future Work

1. **Algorithm Agility**: Dynamic algorithm selection based on security requirements
2. **Hybrid Signatures**: Combine classical and post-quantum algorithms
3. **Batch Verification**: Optimize verification of multiple signatures
4. **Hardware Acceleration**: Leverage specialized PQC hardware when available