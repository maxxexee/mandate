//! Unit tests for the policy arithmetic. They live here so `state.rs` stays
//! one screen of logic; everything below calls the same methods the program does.

use crate::state::*;

use anchor_lang::prelude::*;

const DAY: i64 = 86_400;

fn mandate() -> Mandate {
    Mandate {
        owner: Pubkey::new_unique(),
        agent: Pubkey::new_unique(),
        mint: Pubkey::new_unique(),
        per_payment_cap: 2_000_000,
        period_cap: 5_000_000,
        period_seconds: DAY,
        period_start: 1_000,
        spent_in_period: 0,
        expires_at: 1_000_000,
        allowlist: vec![Pubkey::new_unique()],
    }
}

fn rejection(m: &mut Mandate, amount: u64, now: i64) -> String {
    m.charge(amount, now)
        .expect_err("expected the charge to be rejected")
        .to_string()
}

#[test]
fn accumulates_within_the_period() {
    let mut m = mandate();
    m.charge(2_000_000, 1_100).unwrap();
    m.charge(1_000_000, 1_200).unwrap();
    assert_eq!(m.spent_in_period, 3_000_000);
    assert_eq!(m.period_start, 1_000);
}

#[test]
fn rolls_the_period_over_once_it_elapses() {
    let mut m = mandate();
    m.charge(2_000_000, 1_100).unwrap();
    m.charge(2_000_000, 1_200).unwrap();
    m.charge(1_000_000, 1_300).unwrap();
    assert_eq!(m.spent_in_period, 5_000_000);

    m.charge(1_000_000, 1_000 + DAY).unwrap();
    assert_eq!(m.spent_in_period, 1_000_000);
    assert_eq!(m.period_start, 1_000 + DAY);
}

#[test]
fn rejects_a_payment_above_the_per_payment_cap() {
    let mut m = mandate();
    assert!(rejection(&mut m, 2_000_001, 1_100).contains("CapExceeded"));
}

#[test]
fn rejects_a_payment_that_would_exhaust_the_period() {
    let mut m = mandate();
    m.charge(2_000_000, 1_100).unwrap();
    m.charge(2_000_000, 1_200).unwrap();
    assert!(rejection(&mut m, 2_000_000, 1_300).contains("PeriodExhausted"));
    assert_eq!(m.spent_in_period, 4_000_000);
}

#[test]
fn rejects_a_zero_payment() {
    let mut m = mandate();
    assert!(rejection(&mut m, 0, 1_100).contains("ZeroAmount"));
}

#[test]
fn rejects_after_expiry_even_with_headroom() {
    let mut m = mandate();
    assert!(rejection(&mut m, 1_000_000, 1_000_000).contains("Expired"));
}

fn update(per_payment_cap: u64, period_cap: u64, expires_at: i64) -> PolicyUpdate {
    PolicyUpdate {
        per_payment_cap,
        period_cap,
        period_seconds: DAY,
        expires_at,
    }
}

#[test]
fn update_replaces_every_limit() {
    let mut m = mandate();
    m.apply_update(&update(1_000_000, 3_000_000, 900_000), 1_100)
        .unwrap();

    assert_eq!(m.per_payment_cap, 1_000_000);
    assert_eq!(m.period_cap, 3_000_000);
    assert_eq!(m.expires_at, 900_000);
    assert_eq!(m.period_start, 1_000, "the running period keeps its start");
    assert_eq!(m.spent_in_period, 0);
}

#[test]
fn update_rejects_a_period_cap_below_what_was_spent() {
    let mut m = mandate();
    m.charge(2_000_000, 1_100).unwrap();

    let error = m
        .apply_update(&update(1_000_000, 1_500_000, 1_000_000), 1_200)
        .unwrap_err();
    assert!(error.to_string().contains("PeriodCapBelowSpent"));
    assert_eq!(m.period_cap, 5_000_000, "rejected update writes nothing");
}

#[test]
fn update_rejects_incoherent_limits() {
    let mut m = mandate();
    assert!(m
        .apply_update(&update(3_000_000, 2_000_000, 1_000_000), 1_100)
        .unwrap_err()
        .to_string()
        .contains("PerPaymentCapExceedsPeriodCap"));
    assert!(m
        .apply_update(&update(0, 1_000_000, 1_000_000), 1_100)
        .unwrap_err()
        .to_string()
        .contains("ZeroPerPaymentCap"));
    assert!(m
        .apply_update(&update(1_000_000, 1_000_000, 1_000), 1_100)
        .unwrap_err()
        .to_string()
        .contains("ExpiryInPast"));
}

#[test]
fn adds_and_removes_a_recipient() {
    let mut m = mandate();
    let newcomer = Pubkey::new_unique();

    m.add_recipient(newcomer).unwrap();
    assert!(m.allows(&newcomer));

    m.remove_recipient(newcomer).unwrap();
    assert!(!m.allows(&newcomer));
    assert_eq!(m.allowlist.len(), 1, "the original entry survives");
}

#[test]
fn rejects_a_duplicate_recipient_and_an_unknown_removal() {
    let mut m = mandate();
    let existing = m.allowlist[0];

    assert!(m
        .add_recipient(existing)
        .unwrap_err()
        .to_string()
        .contains("RecipientAlreadyAllowed"));
    assert!(m
        .remove_recipient(Pubkey::new_unique())
        .unwrap_err()
        .to_string()
        .contains("RecipientNotAllowed"));
    assert_eq!(m.allowlist.len(), 1);
}

#[test]
fn refuses_to_grow_the_allowlist_past_the_bound() {
    let mut m = mandate();
    m.allowlist = (0..crate::constants::MAX_ALLOWLIST)
        .map(|_| Pubkey::new_unique())
        .collect();

    assert!(m
        .add_recipient(Pubkey::new_unique())
        .unwrap_err()
        .to_string()
        .contains("AllowlistTooLong"));
}

#[test]
fn allows_only_listed_recipients() {
    let m = mandate();
    assert!(m.allows(&m.allowlist[0]));
    assert!(!m.allows(&Pubkey::new_unique()));
}
