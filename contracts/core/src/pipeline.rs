//! The reward pipeline: scan a day through OutLayer, settle it by minting,
//! close the epoch. Each tick is open to anyone, does one step only if it is
//! due, and tips the caller in GM. An early or repeated call does nothing.

use near_sdk::json_types::{U128, U64};
use near_sdk::serde_json::json;
use near_sdk::{env, near, AccountId, Gas, NearToken, Promise, PromiseOrValue};

use crate::outlayer;
use crate::*;

const MINT_GAS: Gas = Gas::from_tgas(60);
const ON_MINTED_GAS: Gas = Gas::from_tgas(15);
const TIP_MINT_GAS: Gas = Gas::from_tgas(10);
const TIP_CALLBACK_GAS: Gas = Gas::from_tgas(10);
const LEADERBOARD_SIZE: usize = 50;

fn event(name: &str, data: near_sdk::serde_json::Value) {
    env::log_str(&format!(
        "EVENT_JSON:{}",
        json!({ "standard": "gmcoin", "version": "1.0.0", "event": name, "data": [data] })
    ));
}

#[near]
impl Contract {
    /// Starts the next x-scan batch for the oldest unscanned day, if due.
    /// Attach 300 TGas. Returns false when there was nothing to do.
    pub fn scan_tick(&mut self) -> PromiseOrValue<bool> {
        self.assert_not_paused();
        let d = self.scan_day;
        let cal = self.calendar();
        if self.now() < cal.day_start_ns(d + 1) + self.config.scan_delay_ns.0 {
            return PromiseOrValue::Value(false);
        }
        let mut day = match self.days.get(&d) {
            Some(day) => day.clone(),
            None => Day::new(d, self.linked.len()),
        };
        if day.status != DayStatus::Scanning {
            return PromiseOrValue::Value(false);
        }
        if day.scan_end == 0 {
            // Nobody linked yet: nothing to scan.
            day.status = DayStatus::Scanned;
            self.days.insert(d, day);
            self.scan_day += 1;
            self.pay_tip();
            return PromiseOrValue::Value(true);
        }
        let Some(module) = self.modules.x_scan.clone() else {
            return PromiseOrValue::Value(false);
        };
        if day.in_flight.len() as u32 >= self.config.scan_max_in_flight {
            return PromiseOrValue::Value(false);
        }
        let batch = if let Some(b) = day.retry.pop() {
            b
        } else if day.scan_cursor < day.scan_end {
            let to = (day.scan_cursor + self.config.scan_batch_size).min(day.scan_end);
            let b = ScanBatch {
                from: day.scan_cursor,
                to,
                attempts: 0,
            };
            day.scan_cursor = to;
            b
        } else {
            return PromiseOrValue::Value(false);
        };

        let users: Vec<_> = (batch.from..batch.to)
            .filter_map(|i| self.linked.get(i).copied())
            .filter_map(|uid| {
                self.users
                    .get(&uid)
                    .and_then(|u| u.x_id.clone().map(|x| json!([uid, x])))
            })
            .collect();
        let s = &self.config.scoring;
        let input = json!({
            "since": cal.day_start_ns(d) / 1_000_000_000,
            "until": cal.day_start_ns(d + 1) / 1_000_000_000,
            "users": users,
            "scoring": s,
        });
        let (from, to) = (batch.from, batch.to);
        day.in_flight.push(batch);
        self.days.insert(d, day);

        let me = env::current_account_id();
        let job = outlayer::request(
            &self.outlayer,
            &module,
            self.modules.secrets.as_ref(),
            &input,
            &me,
            outlayer::CALLBACK_GAS
                .saturating_add(TIP_MINT_GAS)
                .saturating_add(TIP_CALLBACK_GAS),
        )
        .then(
            Self::ext(me)
                .with_static_gas(outlayer::CALLBACK_GAS)
                .with_unused_gas_weight(0)
                .on_scan(d, from, to),
        );
        self.pay_tip();
        PromiseOrValue::Promise(job)
    }

    #[private]
    pub fn on_scan(&mut self, day: DayIndex, from: u32, to: u32) -> bool {
        let out = crate::outlayer::read_output::<ScanOutput>();
        self.apply_scan(day, from, to, out)
    }

    #[private]
    pub fn on_minted(&mut self, batch_id: U64) -> bool {
        let ok = env::promise_result_checked(0, 1024).is_ok();
        self.apply_minted(batch_id, ok)
    }

    /// Settles the oldest scanned day: computes payouts and mints one chunk.
    /// Retries a failed mint batch first. Returns false when nothing is due.
    pub fn settle_day(&mut self) -> PromiseOrValue<bool> {
        self.assert_not_paused();
        if let Some(batch_id) = self.failed_mints.pop() {
            let batch = self.pending_mints.get(&batch_id).cloned().unwrap();
            let p = self.mint(batch);
            self.pay_tip();
            return PromiseOrValue::Promise(p);
        }
        let d = self.settle_day;
        let Some(mut day) = self.days.get(&d).cloned() else {
            return PromiseOrValue::Value(false);
        };
        let e = self.calendar().epoch_of_day(d);
        if e >= self.epochs.len() {
            return PromiseOrValue::Value(false); // waiting for close_epoch
        }
        match day.status {
            DayStatus::Scanning | DayStatus::Settled | DayStatus::Skipped => {
                return PromiseOrValue::Value(false)
            }
            DayStatus::Failed => {
                // Clean up recorded scores; the pool is not minted.
                let end = (day.settle_cursor + self.config.settle_chunk * 4).min(day.score_count);
                for i in day.settle_cursor..end {
                    self.scores.remove(&(d, i));
                }
                day.settle_cursor = end;
                if end >= day.score_count {
                    day.status = DayStatus::Skipped;
                    self.settle_day += 1;
                    event("day_skipped", json!({ "day": d, "reason": "scan failed" }));
                }
                self.days.insert(d, day);
                self.pay_tip();
                return PromiseOrValue::Value(true);
            }
            DayStatus::Scanned => {
                if day.total_points == 0 {
                    day.status = DayStatus::Skipped;
                    self.days.insert(d, day);
                    self.settle_day += 1;
                    event("day_skipped", json!({ "day": d, "reason": "no points" }));
                    self.pay_tip();
                    return PromiseOrValue::Value(true);
                }
                let epoch = self.epochs.get(e).unwrap();
                let pool = epochs::daily_pool(epoch.budget.0, self.days_per_epoch, self.dust);
                self.dust = 0;
                let split = epochs::split_pool(
                    pool,
                    self.config.emission.treasury_bps,
                    self.tips_unsettled,
                );
                self.tips_unsettled = 0;
                day.pool = U128(pool);
                day.treasury = U128(split.treasury);
                day.users_pool = U128(split.users_pool);
                day.tips = U128(pool - split.treasury - split.users_pool);
                day.status = DayStatus::Settling;
            }
            DayStatus::Settling => {}
        }

        let mut entries: Vec<(Option<UserId>, AccountId, U128)> = Vec::new();
        if day.settle_cursor == 0 && day.treasury.0 > 0 {
            let amount = self.clamp_supply(day.treasury.0);
            if amount > 0 {
                entries.push((None, self.treasury.clone(), U128(amount)));
                day.minted.0 += amount;
            }
        }
        let end = (day.settle_cursor + self.config.settle_chunk).min(day.score_count);
        for i in day.settle_cursor..end {
            let Some((uid, points)) = self.scores.remove(&(d, i)) else {
                continue;
            };
            let raw = epochs::payout(day.users_pool.0, points, day.total_points);
            day.assigned.0 += raw;
            let Some(mut user) = self.users.get(&uid).cloned() else {
                continue;
            };
            let mut amount = raw;
            if !user.verified_human {
                amount = epochs::apply_unverified_cap(
                    amount,
                    user.minted_unverified.0,
                    self.config.unverified_cap.0,
                );
            }
            let amount = self.clamp_supply(amount);
            if amount == 0 {
                continue;
            }
            if !user.verified_human {
                user.minted_unverified.0 += amount;
            }
            user.accrued.0 += amount;
            entries.push((Some(uid), user.near_account.clone(), U128(amount)));
            day.minted.0 += amount;
            self.users.insert(uid, user);
        }
        day.settle_cursor = end;
        let minted_now: u128 = entries.iter().map(|e| e.2 .0).sum();
        if let Some(mut epoch) = self.epochs.get(e).cloned() {
            epoch.minted.0 += minted_now;
            if end >= day.score_count {
                epoch.total_points += day.total_points;
            }
            self.epochs.replace(e, epoch);
        }
        if end >= day.score_count {
            day.status = DayStatus::Settled;
            self.dust += day.users_pool.0.saturating_sub(day.assigned.0);
            self.settle_day += 1;
            event(
                "day_settled",
                json!({ "day": d, "pool": day.pool, "minted": day.minted }),
            );
        }
        self.days.insert(d, day);
        self.pay_tip();
        if entries.is_empty() {
            return PromiseOrValue::Value(true);
        }
        let batch = MintBatch {
            id: U64(self.next_batch_id),
            day: d,
            entries,
        };
        self.next_batch_id += 1;
        self.pending_mints.insert(batch.id.0, batch.clone());
        PromiseOrValue::Promise(self.mint(batch))
    }

    /// Closes the current epoch once all its days are settled, and sets the
    /// next week's budget by the emission rules. Returns false when not due.
    pub fn close_epoch(&mut self) -> bool {
        self.assert_not_paused();
        let e = self.epochs.len() - 1;
        let cal = self.calendar();
        let next_start = cal.first_day_of_epoch(e + 1);
        if self.today() < next_start || self.settle_day < next_start {
            return false;
        }
        let mut cur = self.epochs.get(e).cloned().unwrap();
        let active = self.active_counts.get(&e).copied().unwrap_or(0);
        let recent: Vec<u128> = (e.saturating_sub(3)..=e)
            .filter_map(|i| self.epochs.get(i).map(|x| x.budget.0))
            .collect();
        let budget = epochs::next_budget(
            &self.config.emission.to_config(),
            &epochs::BudgetInput {
                next_index: e + 1,
                prev_budget: cur.budget.0,
                active_users: active,
                recent_budgets: &recent,
            },
        );
        cur.closed = true;
        cur.active_users = active;
        self.epochs.replace(e, cur);
        self.epochs.push(Epoch {
            index: e + 1,
            start_day: next_start,
            budget: U128(budget),
            daily_pool: U128(budget / self.days_per_epoch as u128),
            active_users: 0,
            total_points: 0,
            minted: U128(0),
            closed: false,
        });
        event(
            "epoch_closed",
            json!({ "epoch": e, "active_users": active, "next_budget": U128(budget) }),
        );
        self.pay_tip();
        true
    }
}

impl Contract {
    pub(crate) fn apply_scan(
        &mut self,
        day: DayIndex,
        from: u32,
        to: u32,
        out: Option<ScanOutput>,
    ) -> bool {
        let Some(mut d) = self.days.get(&day).cloned() else {
            return false;
        };
        if d.status != DayStatus::Scanning {
            return false;
        }
        let Some(pos) = d
            .in_flight
            .iter()
            .position(|b| b.from == from && b.to == to)
        else {
            return false;
        };
        let mut batch = d.in_flight.remove(pos);

        let ok = match out {
            Some(out) if out.ok => {
                let allowed: Vec<UserId> = (from..to)
                    .filter_map(|i| self.linked.get(i).copied())
                    .collect();
                let mut seen: Vec<UserId> = Vec::new();
                for (uid, points) in out.scores {
                    if points == 0 || !allowed.contains(&uid) || seen.contains(&uid) {
                        continue;
                    }
                    seen.push(uid);
                    self.record_score(&mut d, uid, points);
                }
                true
            }
            _ => false,
        };
        if !ok {
            batch.attempts += 1;
            if batch.attempts >= self.config.scan_max_attempts {
                d.status = DayStatus::Failed;
                d.in_flight.clear();
                d.retry.clear();
                self.scan_day = day + 1;
                event("day_failed", json!({ "day": day }));
            } else {
                d.retry.push(batch);
            }
        }
        if d.status == DayStatus::Scanning
            && d.scan_cursor >= d.scan_end
            && d.in_flight.is_empty()
            && d.retry.is_empty()
        {
            d.status = DayStatus::Scanned;
            self.scan_day = day + 1;
            event(
                "day_scanned",
                json!({ "day": day, "total_points": d.total_points }),
            );
        }
        self.days.insert(day, d);
        ok
    }

    pub(crate) fn apply_minted(&mut self, batch_id: U64, ok: bool) -> bool {
        let Some(batch) = self.pending_mints.get(&batch_id.0).cloned() else {
            return false;
        };
        if !ok {
            if !self.failed_mints.contains(&batch_id.0) {
                self.failed_mints.push(batch_id.0);
            }
            event("mint_failed", json!({ "batch_id": batch_id }));
            return false;
        }
        for (uid, _, amount) in &batch.entries {
            if let Some(uid) = uid {
                if let Some(mut user) = self.users.get(uid).cloned() {
                    user.delivered.0 += amount.0;
                    self.users.insert(*uid, user);
                }
            }
        }
        self.pending_mints.remove(&batch_id.0);
        true
    }

    fn record_score(&mut self, day: &mut Day, uid: UserId, points: u64) {
        let d = day.index;
        self.scores.insert((d, day.score_count), (uid, points));
        day.score_count += 1;
        day.total_points += points;

        let e = self.calendar().epoch_of_day(d);
        let Some(mut user) = self.users.get(&uid).cloned() else {
            return;
        };
        if user.active_epoch != e {
            user.active_epoch = e;
            user.active_days = 0;
        }
        user.active_days = user.active_days.saturating_add(1);
        if user.active_days == self.config.emission.active_min_days {
            *self.active_counts.entry(e).or_insert(0) += 1;
        }
        if user.points_epoch != e {
            user.points_epoch = e;
            user.epoch_points = 0;
        }
        user.epoch_points += points;
        user.last_scored_day = Some(d);
        user.last_day_points = points;
        let total = user.epoch_points;
        self.users.insert(uid, user);

        let board = self.leaderboards.entry(e).or_default();
        match board.iter_mut().find(|(u, _)| *u == uid) {
            Some(entry) => entry.1 = total,
            None => board.push((uid, total)),
        }
        board.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        board.truncate(LEADERBOARD_SIZE);
    }

    fn clamp_supply(&mut self, amount: u128) -> u128 {
        let amount =
            epochs::apply_max_supply(amount, self.total_minted, self.config.emission.max_supply.0);
        self.total_minted += amount;
        amount
    }

    fn mint(&self, batch: MintBatch) -> Promise {
        let entries: Vec<(AccountId, U128)> = batch
            .entries
            .iter()
            .map(|(_, a, n)| (a.clone(), *n))
            .collect();
        let args = json!({ "batch_id": batch.id.0, "entries": entries })
            .to_string()
            .into_bytes();
        Promise::new(self.token.clone())
            .function_call("mint_batch", args, NearToken::from_yoctonear(0), MINT_GAS)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(ON_MINTED_GAS)
                    .with_unused_gas_weight(0)
                    .on_minted(batch.id),
            )
    }

    /// Mints the tick tip to the caller, within the daily tip budget.
    fn pay_tip(&mut self) {
        let mut counter: caps::DailyCounter = self.tips.into();
        let tip = caps::tip(
            self.today(),
            self.config.tip_amount.0,
            &mut counter,
            self.config.tip_daily_limit.0,
        );
        self.tips = counter.into();
        let tip = self.clamp_supply(tip);
        if tip == 0 {
            return;
        }
        self.tips_unsettled += tip;
        let caller = env::predecessor_account_id();
        let batch = MintBatch {
            id: U64(self.next_batch_id),
            day: self.today(),
            entries: vec![(None, caller, U128(tip))],
        };
        self.next_batch_id += 1;
        self.pending_mints.insert(batch.id.0, batch.clone());
        let entries: Vec<(AccountId, U128)> = batch
            .entries
            .iter()
            .map(|(_, a, n)| (a.clone(), *n))
            .collect();
        let args = json!({ "batch_id": batch.id.0, "entries": entries })
            .to_string()
            .into_bytes();
        Promise::new(self.token.clone())
            .function_call(
                "mint_batch",
                args,
                NearToken::from_yoctonear(0),
                TIP_MINT_GAS,
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(TIP_CALLBACK_GAS)
                    .with_unused_gas_weight(0)
                    .on_minted(batch.id),
            )
            .detach();
    }
}
