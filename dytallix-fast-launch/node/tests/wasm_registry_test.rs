use dytallix_fast_node::runtime::wasm::WasmRuntime;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
enum Method {
    RegisterAsset { hash: String, uri: String },
    GetAsset { id: u64 },
}

#[derive(Deserialize, Debug, PartialEq)]
struct Asset {
    id: u64,
    owner: String,
    hash: String,
    uri: String,
}

#[test]
fn test_registry_contract() {
    // 1. Initialize Runtime
    let runtime = WasmRuntime::new();

    // 2. Load WASM
    let wasm_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/wasm32-unknown-unknown/release/dytallix_registry.wasm");
    
    let wasm_bytes = std::fs::read(&wasm_path).expect("Failed to read WASM file");

    // 3. Deploy
    let deployment = runtime
        .deploy_contract(&wasm_bytes, "deployer_addr", 1_000_000, None)
        .expect("Deployment failed");
    
    println!("Deployed at: {}", deployment.address);

    // 4. Register Asset
    let register_args = Method::RegisterAsset {
        hash: "hash123".to_string(),
        uri: "ipfs://test".to_string(),
    };
    let input = serde_json::to_vec(&register_args).unwrap();
    
    let exec = runtime
        .execute_contract(&deployment.address, "handle", &input, 1_000_000)
        .expect("Execution failed");
    
    let asset_id: u64 = serde_json::from_slice(&exec.result).expect("Failed to parse asset ID");
    assert_eq!(asset_id, 1);

    // 5. Get Asset
    let get_args = Method::GetAsset { id: 1 };
    let input_get = serde_json::to_vec(&get_args).unwrap();
    
    let exec_get = runtime
        .execute_contract(&deployment.address, "handle", &input_get, 1_000_000)
        .expect("Get execution failed");
    
    let asset: Asset = serde_json::from_slice(&exec_get.result).expect("Failed to parse asset");
    
    assert_eq!(asset.id, 1);
    assert_eq!(asset.hash, "hash123");
    assert_eq!(asset.uri, "ipfs://test");
    
    println!("Test passed! Asset retrieved: {:?}", asset);
}
