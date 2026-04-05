use super::PQC;
use fips204::ml_dsa_87; // ML-DSA-87 is the FIPS 204 parameter set corresponding to Dilithium5
use fips204::traits::{KeyGen, SerDes, Signer, Verifier};
use rand_core::OsRng;

pub struct Dilithium;

impl PQC for Dilithium {
    const ALG: &'static str = "dilithium5";
    
    fn keypair() -> (Vec<u8>, Vec<u8>) {
        let (pk, sk) = ml_dsa_87::KG::try_keygen_with_rng(&mut OsRng).expect("keygen failed");
        (sk.into_bytes().to_vec(), pk.into_bytes().to_vec())
    }
    
    fn sign(sk: &[u8], msg: &[u8]) -> Vec<u8> {
        let sk_array: [u8; ml_dsa_87::SK_LEN] = sk.try_into().expect("invalid sk length");
        let sk_obj = ml_dsa_87::PrivateKey::try_from_bytes(sk_array).expect("invalid sk");
        let sig = sk_obj.try_sign(msg, &[]).expect("signing failed");
        sig.to_vec()
    }
    
    fn verify(pk: &[u8], msg: &[u8], sig: &[u8]) -> bool {
        let pk_array: [u8; ml_dsa_87::PK_LEN] = match pk.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let pk_obj = match ml_dsa_87::PublicKey::try_from_bytes(pk_array) {
            Ok(p) => p,
            Err(_) => return false,
        };
        
        let sig_array: [u8; ml_dsa_87::SIG_LEN] = match sig.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        
        pk_obj.verify(msg, &sig_array, &[])
    }
}
