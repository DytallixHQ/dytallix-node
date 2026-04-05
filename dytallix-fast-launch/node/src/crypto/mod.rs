#[allow(clippy::upper_case_acronyms)]
pub trait PQC {
    fn keypair() -> (Vec<u8>, Vec<u8>); // (sk, pk)
    fn sign(sk: &[u8], msg: &[u8]) -> Vec<u8>;
    fn verify(pk: &[u8], msg: &[u8], sig: &[u8]) -> bool;
    const ALG: &'static str;
}

#[cfg(feature = "pqc-real")]
mod dilithium;
#[cfg(feature = "pqc-real")]
pub use dilithium::Dilithium as ActivePQC;

#[cfg(feature = "pqc-fips204")]
mod dilithium_fips204;
#[cfg(feature = "pqc-fips204")]
pub use dilithium_fips204::Dilithium as ActivePQC;

#[cfg(all(feature = "pqc-mock", not(any(feature = "pqc-real", feature = "pqc-fips204"))))]
mod mock;
#[cfg(all(feature = "pqc-mock", not(any(feature = "pqc-real", feature = "pqc-fips204"))))]
pub use mock::MockPQC as ActivePQC;

// New multi-algorithm PQC verification module
pub mod pqc_verify;
pub use pqc_verify::{verify, verify_default, PQCAlgorithm, PQCVerifyError};

mod hash;
pub use hash::{canonical_json, sha3_256};
