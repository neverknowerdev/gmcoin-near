//! Test double of OutLayer's `request_execution`. Tests queue the module
//! outputs it should return; each call pops one. `null` is what real OutLayer
//! returns for a failed or timed-out run; `panic` makes the call itself fail.
//! Every request is recorded so tests can check what core sent.
//!
//! It also acts as a NEP-141 receiver (`ft_on_transfer`) standing in for a DEX,
//! to test the token's fee on `ft_transfer_call`.

use near_sdk::json_types::U128;
use near_sdk::serde_json::Value;
use near_sdk::{env, near, AccountId, Gas, NearToken, PanicOnDefault, Promise};

/// Fee kept from each deposit, like OutLayer's base fee.
const FEE: u128 = 1_000_000_000_000_000_000_000; // 0.001 NEAR

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub enum Scripted {
    /// Return this JSON as the module output.
    Output(String),
    /// Return `null` (failed run).
    Null,
    /// Panic, so the caller's callback sees a failed promise.
    Panic,
}

#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Recorded {
    pub predecessor: AccountId,
    pub source: String,
    pub input_data: Option<String>,
    pub secrets_ref: Option<String>,
    pub payer: Option<AccountId>,
    pub deposit: U128,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    queue: Vec<Scripted>,
    requests: Vec<Recorded>,
    received: Vec<(AccountId, U128)>,
}

#[near]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self {
            queue: vec![],
            requests: vec![],
            received: vec![],
        }
    }

    pub fn push(&mut self, items: Vec<Scripted>) {
        self.queue.extend(items);
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.requests.clear();
    }

    #[payable]
    #[allow(clippy::too_many_arguments)]
    pub fn request_execution(
        &mut self,
        source: Value,
        resource_limits: Option<Value>,
        input_data: Option<String>,
        secrets_ref: Option<Value>,
        response_format: Option<Value>,
        payer_account_id: Option<AccountId>,
        params: Option<Value>,
    ) {
        let _ = (resource_limits, response_format, params);
        let deposit = env::attached_deposit().as_yoctonear();
        let payer = payer_account_id
            .clone()
            .unwrap_or_else(env::predecessor_account_id);
        self.requests.push(Recorded {
            predecessor: env::predecessor_account_id(),
            source: source.to_string(),
            input_data,
            secrets_ref: secrets_ref.map(|s| s.to_string()),
            payer: payer_account_id,
            deposit: U128(deposit),
        });
        if deposit > FEE {
            Promise::new(payer)
                .transfer(NearToken::from_yoctonear(deposit - FEE))
                .detach();
        }
        let next = if self.queue.is_empty() {
            Scripted::Null
        } else {
            self.queue.remove(0)
        };
        match next {
            Scripted::Output(json) => env::value_return(json.as_bytes()),
            Scripted::Null => env::value_return(b"null"),
            // Panicking here would revert the queue pop, so fail in a
            // follow-up receipt whose result becomes this call's result.
            Scripted::Panic => {
                let _ = Promise::new(env::current_account_id())
                    .function_call(
                        "fail_now",
                        vec![],
                        NearToken::from_yoctonear(0),
                        Gas::from_tgas(5),
                    )
                    .as_return();
            }
        }
    }

    #[private]
    pub fn fail_now(&self) {
        env::panic_str("scripted failure");
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.clone()
    }

    /// NEP-141 receiver: keeps the amount minus the refund given in `msg`.
    pub fn ft_on_transfer(&mut self, sender_id: AccountId, amount: U128, msg: String) -> U128 {
        self.received.push((sender_id, amount));
        U128(msg.parse().unwrap_or(0))
    }

    pub fn received(&self) -> Vec<(AccountId, U128)> {
        self.received.clone()
    }

    pub fn queued(&self) -> u32 {
        self.queue.len() as u32
    }
}
