//! Free view calls for the app, indexers and analytics.

use near_sdk::json_types::{U128, U64};
use near_sdk::{near, AccountId};

use crate::*;

#[near(serializers = [json])]
pub struct Status {
    pub now: U64,
    pub today: DayIndex,
    pub current_epoch: EpochIndex,
    pub budget: U128,
    pub daily_pool: U128,
    pub scan_day: DayIndex,
    pub settle_day: DayIndex,
    pub scan_due: bool,
    pub settle_due: bool,
    pub close_due: bool,
    pub users: u64,
    pub linked: u32,
    pub total_minted: U128,
    pub paused: bool,
    pub failed_mints: u32,
}

#[near(serializers = [json])]
pub struct Estimate {
    pub day: Option<DayIndex>,
    pub points: u64,
    pub day_points: u64,
    pub estimate: U128,
}

#[near(serializers = [json])]
pub struct Settings {
    pub owner: AccountId,
    pub token: AccountId,
    pub treasury: AccountId,
    pub outlayer: AccountId,
    pub config: Config,
    pub modules: Modules,
    pub genesis_ns: U64,
    pub day_ns: U64,
    pub days_per_epoch: u32,
}

#[near]
impl Contract {
    pub fn get_settings(&self) -> Settings {
        Settings {
            owner: self.owner.clone(),
            token: self.token.clone(),
            treasury: self.treasury.clone(),
            outlayer: self.outlayer.clone(),
            config: self.config.clone(),
            modules: self.modules.clone(),
            genesis_ns: U64(self.genesis_ns),
            day_ns: U64(self.day_ns),
            days_per_epoch: self.days_per_epoch,
        }
    }

    pub fn status(&self) -> Status {
        let cal = self.calendar();
        let today = self.today();
        let e = self.epochs.len() - 1;
        let epoch = self.epochs.get(e).unwrap();
        let scan_due =
            self.now() >= cal.day_start_ns(self.scan_day + 1) + self.config.scan_delay_ns.0;
        let settle_due = !self.failed_mints.is_empty()
            || self.days.get(&self.settle_day).is_some_and(|d| {
                !matches!(
                    d.status,
                    DayStatus::Scanning | DayStatus::Settled | DayStatus::Skipped
                ) && cal.epoch_of_day(self.settle_day) < self.epochs.len()
            });
        let next_start = cal.first_day_of_epoch(e + 1);
        Status {
            now: U64(self.now()),
            today,
            current_epoch: e,
            budget: epoch.budget,
            daily_pool: epoch.daily_pool,
            scan_day: self.scan_day,
            settle_day: self.settle_day,
            scan_due,
            settle_due,
            close_due: today >= next_start && self.settle_day >= next_start,
            users: self.user_count,
            linked: self.linked.len(),
            total_minted: U128(self.total_minted),
            paused: self.paused,
            failed_mints: self.failed_mints.len() as u32,
        }
    }

    pub fn get_user(&self, account_id: AccountId) -> Option<User> {
        self.by_account
            .get(&account_id)
            .and_then(|id| self.users.get(id))
            .cloned()
    }

    pub fn get_user_by_id(&self, user_id: U64) -> Option<User> {
        self.users.get(&user_id.0).cloned()
    }

    pub fn get_user_by_x(&self, x_id: String) -> Option<User> {
        self.by_x
            .get(&x_id)
            .and_then(|id| self.users.get(id))
            .cloned()
    }

    pub fn get_pending_link(&self, account_id: AccountId) -> Option<PendingLink> {
        self.by_account
            .get(&account_id)
            .and_then(|id| self.pending_links.get(id))
            .cloned()
    }

    pub fn get_day(&self, day: DayIndex) -> Option<Day> {
        self.days.get(&day).cloned()
    }

    pub fn get_epoch(&self, epoch: EpochIndex) -> Option<Epoch> {
        self.epochs.get(epoch).cloned()
    }

    /// Epochs from `from`, oldest first: the budget curve for the app.
    pub fn epoch_history(&self, from: Option<EpochIndex>, limit: Option<u32>) -> Vec<Epoch> {
        let from = from.unwrap_or(0);
        let limit = limit.unwrap_or(52).min(200);
        (from..self.epochs.len())
            .take(limit as usize)
            .filter_map(|i| self.epochs.get(i).cloned())
            .collect()
    }

    /// Same data as `epoch_history`, named for indexers.
    pub fn export_epochs(&self, from: Option<EpochIndex>, limit: Option<u32>) -> Vec<Epoch> {
        self.epoch_history(from, limit)
    }

    /// Users by id from `from_id`, for indexers and analytics.
    pub fn export_users(&self, from_id: Option<U64>, limit: Option<u32>) -> Vec<User> {
        let from = from_id.map(|x| x.0).unwrap_or(1).max(1);
        let limit = limit.unwrap_or(100).min(500) as u64;
        (from..self.next_user_id)
            .filter_map(|id| self.users.get(&id).cloned())
            .take(limit as usize)
            .collect()
    }

    /// Top users of an epoch by points: (account, X handle, points).
    pub fn leaderboard(&self, epoch: Option<EpochIndex>) -> Vec<(AccountId, Option<String>, u64)> {
        let e = epoch.unwrap_or(self.epochs.len() - 1);
        self.leaderboards
            .get(&e)
            .map(|b| {
                b.iter()
                    .filter_map(|(uid, pts)| {
                        self.users
                            .get(uid)
                            .map(|u| (u.near_account.clone(), u.x_handle.clone(), *pts))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The user's share of the latest scored day not yet settled:
    /// pool x my points / points so far.
    pub fn estimate_today(&self, account_id: AccountId) -> Estimate {
        let none = Estimate {
            day: None,
            points: 0,
            day_points: 0,
            estimate: U128(0),
        };
        let Some(user) = self.get_user(account_id) else {
            return none;
        };
        let Some(d) = user.last_scored_day else {
            return none;
        };
        let Some(day) = self.days.get(&d) else {
            return none;
        };
        if matches!(
            day.status,
            DayStatus::Settled | DayStatus::Skipped | DayStatus::Failed
        ) {
            return none;
        }
        let e = self.calendar().epoch_of_day(d);
        let budget = self
            .epochs
            .get(e)
            .map(|x| x.budget.0)
            .unwrap_or(self.epochs.get(self.epochs.len() - 1).unwrap().budget.0);
        let pool = epochs::daily_pool(budget, self.days_per_epoch, 0);
        let split = epochs::split_pool(pool, self.config.emission.treasury_bps, 0);
        Estimate {
            day: Some(d),
            points: user.last_day_points,
            day_points: day.total_points,
            estimate: U128(epochs::payout(
                split.users_pool,
                user.last_day_points,
                day.total_points,
            )),
        }
    }
}
