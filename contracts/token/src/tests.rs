use super::*;
use near_sdk::test_utils::{accounts, VMContextBuilder};
use near_sdk::testing_env;

const GM: u128 = 1_000_000_000_000_000_000;

fn owner() -> AccountId {
    accounts(0)
}
fn core() -> AccountId {
    "core.gm.testnet".parse().unwrap()
}
fn treasury() -> AccountId {
    "treasury.gm.testnet".parse().unwrap()
}
fn alice() -> AccountId {
    accounts(1)
}
fn bob() -> AccountId {
    accounts(2)
}

fn ctx(predecessor: AccountId, deposit: u128) -> VMContextBuilder {
    let mut b = VMContextBuilder::new();
    b.predecessor_account_id(predecessor)
        .attached_deposit(NearToken::from_yoctonear(deposit))
        .account_balance(NearToken::from_near(10))
        .block_timestamp(1_000 * DAY_NS);
    b
}

fn setup(fee_bps: u16) -> Contract {
    testing_env!(ctx(owner(), 0).build());
    Contract::new(
        owner(),
        core(),
        treasury(),
        default_metadata(),
        U128(1_000_000 * GM),
        U128(10_000 * GM),
        fee_bps,
    )
}

fn mint(c: &mut Contract, batch: u64, entries: &[(AccountId, u128)]) {
    testing_env!(ctx(core(), 0).build());
    c.mint_batch(
        batch,
        entries.iter().map(|(a, n)| (a.clone(), U128(*n))).collect(),
    );
}

#[test]
fn metadata_and_init() {
    let c = setup(100);
    assert_eq!(c.ft_metadata().decimals, 18);
    assert_eq!(c.ft_metadata().symbol, "GM");
    assert_eq!(c.ft_total_supply().0, 0);
    assert!(c.is_fee_exempt(treasury()));
    assert_eq!(c.ft_balance_of(treasury()).0, 0);
}

#[test]
#[should_panic(expected = "fee above 5%")]
fn fee_is_hard_capped_at_init() {
    testing_env!(ctx(owner(), 0).build());
    Contract::new(
        owner(),
        core(),
        treasury(),
        default_metadata(),
        U128(1),
        U128(1),
        501,
    );
}

#[test]
fn minter_mints_and_registers_receivers() {
    let mut c = setup(0);
    mint(
        &mut c,
        1,
        &[(alice(), 5 * GM), (bob(), 7 * GM), (treasury(), GM)],
    );
    assert_eq!(c.ft_balance_of(alice()).0, 5 * GM);
    assert_eq!(c.ft_balance_of(bob()).0, 7 * GM);
    assert_eq!(c.ft_total_supply().0, 13 * GM);
    assert!(c.is_batch_used(1));
    assert_eq!(c.get_config().minted_today.0, 13 * GM);
}

#[test]
#[should_panic(expected = "batch already minted")]
fn replayed_batch_is_refused() {
    let mut c = setup(0);
    mint(&mut c, 1, &[(alice(), GM)]);
    mint(&mut c, 1, &[(alice(), GM)]);
}

#[test]
#[should_panic(expected = "only minter")]
fn only_minter_mints() {
    let mut c = setup(0);
    testing_env!(ctx(owner(), 0).build());
    c.mint_batch(1, vec![(alice(), U128(GM))]);
}

#[test]
#[should_panic(expected = "paused")]
fn pause_stops_mints() {
    let mut c = setup(0);
    testing_env!(ctx(owner(), 0).build());
    c.set_paused(true);
    mint(&mut c, 1, &[(alice(), GM)]);
}

#[test]
#[should_panic(expected = "daily mint cap exceeded")]
fn daily_cap_holds() {
    let mut c = setup(0);
    mint(&mut c, 1, &[(alice(), 6_000 * GM)]);
    mint(&mut c, 2, &[(alice(), 5_000 * GM)]);
}

#[test]
fn daily_cap_resets_next_day() {
    let mut c = setup(0);
    mint(&mut c, 1, &[(alice(), 10_000 * GM)]);
    let mut b = ctx(core(), 0);
    b.block_timestamp(1_001 * DAY_NS);
    testing_env!(b.build());
    c.mint_batch(2, vec![(alice(), U128(10_000 * GM))]);
    assert_eq!(c.ft_balance_of(alice()).0, 20_000 * GM);
}

#[test]
#[should_panic(expected = "max supply exceeded")]
fn max_supply_holds() {
    let mut c = setup(0);
    testing_env!(ctx(owner(), 0).build());
    c.set_daily_mint_cap(U128(u128::MAX / 2));
    mint(&mut c, 1, &[(alice(), 999_999 * GM)]);
    mint(&mut c, 2, &[(alice(), 2 * GM)]);
}

#[test]
fn transfer_takes_fee_to_treasury() {
    let mut c = setup(100); // 1%
    mint(&mut c, 1, &[(alice(), 100 * GM)]);
    testing_env!(ctx(owner(), 1_250_000_000_000_000_000_000).build());
    c.storage_deposit(Some(bob()), None);
    testing_env!(ctx(alice(), 1).build());
    c.ft_transfer(bob(), U128(10 * GM), None);
    assert_eq!(c.ft_balance_of(alice()).0, 90 * GM);
    assert_eq!(c.ft_balance_of(bob()).0, 10 * GM - GM / 10);
    assert_eq!(c.ft_balance_of(treasury()).0, GM / 10);
    assert_eq!(c.ft_total_supply().0, 100 * GM);
}

#[test]
fn exempt_accounts_pay_no_fee() {
    let mut c = setup(500);
    mint(&mut c, 1, &[(alice(), 100 * GM), (bob(), 1)]);
    testing_env!(ctx(owner(), 0).build());
    c.set_fee_exempt(bob(), true);
    testing_env!(ctx(alice(), 1).build());
    c.ft_transfer(bob(), U128(10 * GM), None);
    assert_eq!(c.ft_balance_of(bob()).0, 10 * GM + 1);
    assert_eq!(c.fee_for(alice(), accounts(3), U128(100)).0, 5);
    assert_eq!(c.fee_for(alice(), bob(), U128(100)).0, 0);
}

#[test]
#[should_panic(expected = "fee above 5%")]
fn fee_setter_is_capped() {
    let mut c = setup(0);
    testing_env!(ctx(core(), 0).build());
    c.set_fee_bps(501);
}

#[test]
#[should_panic(expected = "only owner or minter")]
fn strangers_cannot_change_settings() {
    let mut c = setup(0);
    testing_env!(ctx(alice(), 0).build());
    c.set_paused(true);
}

#[test]
fn minter_can_be_switched() {
    let mut c = setup(0);
    testing_env!(ctx(owner(), 0).build());
    c.set_minter(alice());
    testing_env!(ctx(alice(), 0).build());
    c.mint_batch(9, vec![(bob(), U128(GM))]);
    assert_eq!(c.ft_balance_of(bob()).0, GM);
}

#[test]
fn zero_entries_are_skipped() {
    let mut c = setup(0);
    mint(&mut c, 1, &[(alice(), 0)]);
    assert!(c.storage_balance_of(alice()).is_none());
}
