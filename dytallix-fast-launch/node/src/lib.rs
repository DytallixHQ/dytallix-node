pub mod addr; // address derivation
pub mod alerts; // alerting subsystem
pub mod crypto; // new crypto module
pub mod execution; // deterministic execution engine
pub mod gas; // gas accounting system
pub mod mempool;
pub mod metrics; // observability module (internally feature-gated)
pub mod p2p;
pub mod rpc;
pub mod runtime;
// Expose governance module unconditionally; runtime flags gate behavior
pub use runtime::governance;
#[cfg(feature = "oracle")]
pub use runtime::oracle;
// Expose staking module unconditionally; runtime flags gate behavior
pub use runtime::staking;
pub mod state;
pub mod storage;
pub mod types; // canonical transaction types
pub mod util;
pub mod ws; // added util module // p2p networking and gossip
            // re-export emission types
pub use runtime::emission::*;
pub mod secrets; // vault + sealed keystore providers
