//! Starting OutLayer jobs. Core calls `request_execution` on the OutLayer
//! contract; the result comes back through yield/resume as the promise result,
//! read by core's private callback. A failed or timed-out run returns `null`.

use near_sdk::serde_json::{json, Value};
use near_sdk::{env, AccountId, Gas, GasWeight, NearToken, Promise};

use crate::{ModuleRef, ModuleSource, SecretsRef};

/// Gas kept for core's callback after the OutLayer call.
pub const CALLBACK_GAS: Gas = Gas::from_tgas(40);
/// Gas OutLayer needs at least (it reserves 50 TGas for its own resume callback).
pub const MIN_OUTLAYER_GAS: Gas = Gas::from_tgas(100);

pub fn source_json(source: &ModuleSource) -> Value {
    match source {
        ModuleSource::GitHub {
            repo,
            commit,
            build_target,
        } => json!({
            "GitHub": { "repo": repo, "commit": commit, "build_target": build_target }
        }),
        ModuleSource::WasmUrl {
            url,
            hash,
            build_target,
        } => json!({
            "WasmUrl": { "url": url, "hash": hash, "build_target": build_target }
        }),
    }
}

pub fn request_args(
    module: &ModuleRef,
    secrets: Option<&SecretsRef>,
    input: &Value,
    payer: &AccountId,
) -> Vec<u8> {
    json!({
        "source": source_json(&module.source),
        "resource_limits": {
            "max_instructions": module.max_instructions.0,
            "max_memory_mb": module.max_memory_mb,
            "max_execution_seconds": module.max_execution_seconds.0,
        },
        "input_data": input.to_string(),
        "secrets_ref": secrets.map(|s| json!({ "profile": s.profile, "account_id": s.account_id })),
        "response_format": "Json",
        "payer_account_id": payer,
    })
    .to_string()
    .into_bytes()
}

/// `request_execution` with all gas left after `reserve` (the callback and
/// anything else this call schedules) forwarded to OutLayer.
pub fn request(
    outlayer: &AccountId,
    module: &ModuleRef,
    secrets: Option<&SecretsRef>,
    input: &Value,
    payer: &AccountId,
    reserve: Gas,
) -> Promise {
    let left = env::prepaid_gas()
        .saturating_sub(env::used_gas())
        .saturating_sub(reserve);
    near_sdk::require!(
        left >= MIN_OUTLAYER_GAS,
        "attach more gas (300 TGas recommended)"
    );
    Promise::new(outlayer.clone()).function_call_weight(
        "request_execution",
        request_args(module, secrets, input, payer),
        NearToken::from_yoctonear(module.deposit.0),
        MIN_OUTLAYER_GAS,
        GasWeight(1),
    )
}
