//! Registration and X linking.

use gm_scoring::link_code;
use near_sdk::json_types::{U128, U64};
use near_sdk::serde_json::json;
use near_sdk::{env, near, require, AccountId, NearToken, Promise};

use crate::outlayer;
use crate::*;

/// After this long an in-flight verification counts as lost.
const STALE_LINK_NS: u64 = 15 * 60 * 1_000_000_000;

#[near]
impl Contract {
    /// Creates a user for the caller. Attach `register_deposit` to pay your own
    /// storage (refunded on `unregister`), or attach nothing to be sponsored by
    /// core within its daily caps (gasless onboarding through the relayer).
    #[payable]
    pub fn register(&mut self) -> U64 {
        self.assert_not_paused();
        let account = env::predecessor_account_id();
        require!(
            !self.by_account.contains_key(&account),
            "already registered"
        );
        let attached = env::attached_deposit().as_yoctonear();
        let need = self.config.register_deposit.0;

        let sponsored = if attached == 0 {
            let day = self.today();
            let mut state = self.sponsor_state();
            if let Err(e) = state.register(
                day,
                self.config.sponsored_storage_cost.0,
                &self.sponsor_limits(),
            ) {
                env::panic_str(&format!("sponsorship unavailable: {e:?}"));
            }
            self.save_sponsor_state(state);
            true
        } else {
            require!(attached >= need, "attach register_deposit or nothing");
            if attached > need {
                Promise::new(account.clone())
                    .transfer(NearToken::from_yoctonear(attached - need))
                    .detach();
            }
            false
        };

        let id = self.next_user_id;
        self.next_user_id += 1;
        self.user_count += 1;
        self.users.insert(
            id,
            User {
                id,
                near_account: account.clone(),
                x_id: None,
                x_handle: None,
                accrued: U128(0),
                delivered: U128(0),
                minted_unverified: U128(0),
                verified_human: false,
                sponsored,
                storage_deposit: U128(if sponsored { 0 } else { need }),
                created_at: U64(self.now()),
                linked_at: None,
                checks: Counter::default(),
                active_epoch: 0,
                active_days: 0,
                points_epoch: 0,
                epoch_points: 0,
                last_scored_day: None,
                last_day_points: 0,
            },
        );
        self.by_account.insert(account, id);
        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({
                "standard": "gmcoin", "version": "1.0.0", "event": "register",
                "data": [{ "user_id": id, "sponsored": sponsored }]
            })
        ));
        U64(id)
    }

    /// Removes the caller's user if no X account is linked and refunds the
    /// storage deposit they paid.
    pub fn unregister(&mut self) {
        let account = env::predecessor_account_id();
        let id = self.user_id_of(&account);
        let user = self.users.get(&id).cloned().unwrap();
        require!(user.x_id.is_none(), "unlink X first");
        self.remove_user(&user);
        if user.storage_deposit.0 > 0 {
            Promise::new(account)
                .transfer(NearToken::from_yoctonear(user.storage_deposit.0))
                .detach();
        }
    }

    /// Removes a sponsored user that did not link X within `sponsor_reclaim_after_ns`.
    /// Anyone can call it; storage returns to core.
    pub fn reclaim_sponsored(&mut self, account_id: AccountId) {
        let id = self.user_id_of(&account_id);
        let user = self.users.get(&id).cloned().unwrap();
        require!(user.sponsored && user.x_id.is_none(), "not reclaimable");
        require!(
            self.now() >= user.created_at.0 + self.config.sponsor_reclaim_after_ns.0,
            "too early"
        );
        self.remove_user(&user);
    }

    /// Creates the caller's link code. Post it on X, then call `verify_x_link`.
    pub fn start_x_link(&mut self) -> String {
        self.assert_not_paused();
        let account = env::predecessor_account_id();
        let id = self.user_id_of(&account);
        require!(
            self.users.get(&id).unwrap().x_id.is_none(),
            "X already linked"
        );
        if let Some(p) = self.pending_links.get(&id) {
            // A lost callback must not lock the user out forever.
            require!(
                !p.in_flight || self.now() > p.created_at.0 + STALE_LINK_NS,
                "verification in progress"
            );
        }
        let seed = env::random_seed();
        let code = link_code::generate(account.as_str(), [seed[0], seed[1], seed[2]]);
        self.pending_links.insert(
            id,
            PendingLink {
                code: code.clone(),
                created_at: U64(self.now()),
                in_flight: false,
                last_error: None,
            },
        );
        code
    }

    /// Starts the x-verify OutLayer job: it finds the link code by X search,
    /// or reads the pasted post. Attach the module deposit to pay yourself
    /// (unused part refunded), or nothing to use core's sponsored checks.
    /// Attach 300 TGas.
    #[payable]
    pub fn verify_x_link(&mut self, post_url: Option<String>) -> Promise {
        self.assert_not_paused();
        let account = env::predecessor_account_id();
        let id = self.user_id_of(&account);
        let mut pending = self
            .pending_links
            .get(&id)
            .cloned()
            .unwrap_or_else(|| env::panic_str("call start_x_link first"));
        require!(!pending.in_flight, "verification in progress");
        let module = self
            .modules
            .x_verify
            .clone()
            .unwrap_or_else(|| env::panic_str("x-verify module not set"));
        let post_id = post_url.map(|u| {
            link_code::parse_post_id(&u).unwrap_or_else(|| env::panic_str("bad post link"))
        });

        let attached = env::attached_deposit().as_yoctonear();
        let payer = if attached == 0 {
            let mut user = self.users.get(&id).cloned().unwrap();
            let mut counter: caps::DailyCounter = user.checks.into();
            let mut state = self.sponsor_state();
            if let Err(e) = state.check(
                self.today(),
                module.deposit.0,
                &mut counter,
                &self.sponsor_limits(),
            ) {
                env::panic_str(&format!("sponsorship unavailable: {e:?}"));
            }
            self.save_sponsor_state(state);
            user.checks = counter.into();
            self.users.insert(id, user);
            env::current_account_id()
        } else {
            require!(
                attached >= module.deposit.0,
                "attach the module deposit or nothing"
            );
            if attached > module.deposit.0 {
                Promise::new(account.clone())
                    .transfer(NearToken::from_yoctonear(attached - module.deposit.0))
                    .detach();
            }
            account.clone()
        };

        pending.in_flight = true;
        pending.last_error = None;
        pending.created_at = U64(self.now());
        let code = pending.code.clone();
        self.pending_links.insert(id, pending);

        let input = json!({ "code": code, "account_id": account, "post_id": post_id });
        outlayer::request(
            &self.outlayer,
            &module,
            self.modules.secrets.as_ref(),
            &input,
            &payer,
            outlayer::CALLBACK_GAS,
        )
        .then(
            Self::ext(env::current_account_id())
                .with_static_gas(outlayer::CALLBACK_GAS)
                .with_unused_gas_weight(0)
                .on_x_verified(U64(id), code),
        )
    }

    #[private]
    pub fn on_x_verified(&mut self, user_id: U64, code: String) -> bool {
        let out = crate::outlayer::read_output::<VerifyOutput>();
        self.apply_x_verified(user_id, code, out)
    }
}

impl Contract {
    pub(crate) fn apply_x_verified(
        &mut self,
        user_id: U64,
        code: String,
        out: Option<VerifyOutput>,
    ) -> bool {
        let id = user_id.0;
        let Some(mut pending) = self.pending_links.get(&id).cloned() else {
            return false;
        };
        if pending.code != code {
            return false;
        }
        pending.in_flight = false;
        let out = out.unwrap_or_else(|| VerifyOutput {
            error: Some("OutLayer run failed".into()),
            ..Default::default()
        });
        let error = match (&out.ok, &out.x_id) {
            (true, Some(x_id)) if self.by_x.contains_key(x_id) => {
                Some("X account already linked".into())
            }
            (true, Some(x_id)) if x_id.is_empty() || x_id.len() > 32 => Some("bad X id".into()),
            (true, Some(_)) => None,
            _ => Some(
                out.error
                    .clone()
                    .unwrap_or_else(|| "link code not found".into()),
            ),
        };
        if let Some(e) = error {
            pending.last_error = Some(e);
            self.pending_links.insert(id, pending);
            return false;
        }
        let x_id = out.x_id.unwrap();
        let Some(mut user) = self.users.get(&id).cloned() else {
            return false;
        };
        user.x_id = Some(x_id.clone());
        user.x_handle = out.handle.map(|h| h.chars().take(32).collect());
        user.linked_at = Some(U64(self.now()));
        self.by_x.insert(x_id.clone(), id);
        self.linked.push(id);
        self.users.insert(id, user);
        self.pending_links.remove(&id);
        env::log_str(&format!(
            "EVENT_JSON:{}",
            json!({
                "standard": "gmcoin", "version": "1.0.0", "event": "x_linked",
                "data": [{ "user_id": id, "x_id": x_id, "post_id": out.post_id }]
            })
        ));
        true
    }
}

impl Contract {
    fn remove_user(&mut self, user: &User) {
        self.users.remove(&user.id);
        self.by_account.remove(&user.near_account);
        self.pending_links.remove(&user.id);
        self.user_count -= 1;
    }

    pub(crate) fn sponsor_limits(&self) -> caps::SponsorLimits {
        caps::SponsorLimits {
            checks_per_user_per_day: self.config.sponsor_checks_per_user_per_day,
            daily_budget: self.config.sponsor_daily_budget.0,
            registrations_per_day: self.config.sponsor_registrations_per_day,
        }
    }

    pub(crate) fn sponsor_state(&self) -> caps::SponsorState {
        caps::SponsorState {
            spent: self.sponsor_spent.into(),
            registrations: self.sponsor_registrations.into(),
        }
    }

    pub(crate) fn save_sponsor_state(&mut self, s: caps::SponsorState) {
        self.sponsor_spent = s.spent.into();
        self.sponsor_registrations = s.registrations.into();
    }
}
