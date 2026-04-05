pub mod bridge;
pub mod emission;
pub mod fee_burn;
pub mod governance;
#[cfg(feature = "oracle")]
pub mod oracle;
pub mod staking;
pub mod dead_man_switch;
#[cfg(feature = "contracts")]
pub mod wasm;

#[cfg(test)]
pub mod tests;

// The modules are feature-gated, meaning they will only be compiled
// and included in the project if the corresponding feature is enabled.
// This allows for optional functionality and reduces the binary size
// for users who do not need the extra features.
