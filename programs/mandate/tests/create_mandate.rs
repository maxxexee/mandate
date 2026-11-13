//! `create_mandate` binds a policy to an agent key.

mod support;

use {
    anchor_lang::{prelude::Pubkey, Space},
    mandate::state::Mandate,
    support::*,
};

#[test]
fn creates_a_mandate_that_matches_the_policy() {
    let mut f = Fixture::new();
    let policy = f.policy();
    f.create_mandate();

    let mandate = f.read_mandate();
    assert_eq!(mandate.owner, f.owner.pubkey());
    assert_eq!(mandate.agent, f.agent.pubkey());
    assert_eq!(mandate.mint, f.mint);
    assert_eq!(mandate.per_payment_cap, PER_PAYMENT_CAP);
    assert_eq!(mandate.period_cap, PERIOD_CAP);
    assert_eq!(mandate.period_seconds, PERIOD_SECONDS);
    assert_eq!(mandate.expires_at, EXPIRES_AT);
    assert_eq!(mandate.allowlist, policy.allowlist);
    assert_eq!(mandate.spent_in_period, 0);
}

#[test]
fn account_is_sized_for_the_full_allowlist() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.allowlist = (0..mandate::constants::MAX_ALLOWLIST)
        .map(|_| Pubkey::new_unique())
        .collect();
    let ix = f.create_ix(policy.clone());
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);

    let account = f.svm.get_account(&f.mandate_address()).unwrap();
    assert_eq!(account.data.len(), 8 + Mandate::INIT_SPACE);
    assert_eq!(f.read_mandate().allowlist.len(), policy.allowlist.len());
}

#[test]
fn rejects_a_per_payment_cap_above_the_period_cap() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.per_payment_cap = PERIOD_CAP + 1;
    let ix = f.create_ix(policy);

    f.expect_rejection(
        &[ix],
        &[f.owner.insecure_clone()],
        "PerPaymentCapExceedsPeriodCap",
    );
}

#[test]
fn rejects_a_zero_per_payment_cap() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.per_payment_cap = 0;
    let ix = f.create_ix(policy);

    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "ZeroPerPaymentCap");
}

#[test]
fn rejects_a_zero_period() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.period_seconds = 0;
    let ix = f.create_ix(policy);

    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "ZeroPeriod");
}

#[test]
fn rejects_an_expiry_in_the_past() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.expires_at = -1;
    let ix = f.create_ix(policy);

    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "ExpiryInPast");
}

#[test]
fn rejects_an_empty_allowlist() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.allowlist = Vec::new();
    let ix = f.create_ix(policy);

    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "EmptyAllowlist");
}

#[test]
fn rejects_an_allowlist_over_the_bound() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.allowlist = (0..=mandate::constants::MAX_ALLOWLIST)
        .map(|_| Pubkey::new_unique())
        .collect();
    let ix = f.create_ix(policy);

    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "AllowlistTooLong");
}

#[test]
fn rejects_the_owner_as_its_own_agent() {
    let mut f = Fixture::new();
    let ix = f.create_ix_with_agent(f.owner.pubkey(), f.policy());

    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "AgentIsOwner");
}

#[test]
fn a_second_mandate_for_the_same_pair_cannot_be_created() {
    let mut f = Fixture::new();
    f.create_mandate();

    // A byte-identical transaction is deduped as AlreadyProcessed, so the
    // second attempt carries a different but still valid policy.
    let mut policy = f.policy();
    policy.per_payment_cap = 1_000_000;
    let second = f.create_ix(policy);
    let error = f
        .submit(&[second], &[f.owner.insecure_clone()])
        .expect_err("the mandate PDA must already be in use");
    assert!(
        error.contains("already in use") || error.contains("AccountAlreadyInitialized"),
        "unexpected error:\n{error}"
    );
}
