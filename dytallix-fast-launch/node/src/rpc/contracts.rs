use crate::rpc::{ApiError, RpcContext};
use axum::{
    extract::{Path, Extension},
    Json,
};
use serde_json::json;
#[cfg(feature = "contracts")]
pub async fn contracts_deploy(
    Extension(ctx): Extension<RpcContext>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let code_hex = payload
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or(ApiError::BadRequest("missing code".to_string()))?;
    
    let code = hex::decode(code_hex.trim_start_matches("0x"))
        .map_err(|_| ApiError::BadRequest("invalid hex code".to_string()))?;

    let deployer = payload
        .get("deployer")
        .and_then(|v| v.as_str())
        .unwrap_or("dyt1genesis");

    let gas_limit = payload
        .get("gas_limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(1_000_000);

    let address = ctx.wasm_runtime
        .deploy_contract(&code, deployer, gas_limit, None)
        .map_err(|e| ApiError::Internal)?;

    Ok(Json(json!({
        "address": address.address,
        "tx_hash": address.tx_hash
    })))
}

#[cfg(feature = "contracts")]
pub async fn contracts_call(
    Extension(ctx): Extension<RpcContext>,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let address = payload
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or(ApiError::BadRequest("missing address".to_string()))?;

    let method = payload
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or(ApiError::BadRequest("missing method".to_string()))?;

    let args_hex = payload
        .get("args")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    let args = hex::decode(args_hex.trim_start_matches("0x"))
        .map_err(|_| ApiError::BadRequest("invalid hex args".to_string()))?;

    let gas_limit = payload
        .get("gas_limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(1_000_000);

    let result = ctx.wasm_runtime
        .execute_contract(&address.to_string(), method, &args, gas_limit)
        .map_err(|e| ApiError::Internal)?;

    Ok(Json(json!({
        "result": hex::encode(result.result),
        "gas_used": result.gas_used,
        "logs": result.logs
    })))
}

#[cfg(feature = "contracts")]
pub async fn contracts_state(
    Extension(ctx): Extension<RpcContext>,
    Path((address, key)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // This is a placeholder as WasmRuntime doesn't expose state query directly yet
    // We might need to add a method to WasmRuntime to query state
    Err(ApiError::NotImplemented("state query not implemented".to_string()))
}
