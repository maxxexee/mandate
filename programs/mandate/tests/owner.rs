//! Owner-side edits: the policy and the allowlist can change without recreating
//! the mandate, and the agent cannot do either.

mod support;

use {anchor_lang::prelude::Pubkey, mandate::state::PolicyUpdate, support::*};

fn update(per_payment_cap: u64, period_cap: u64, expires_at: i64) -> PolicyUpdate {
    PolicyUpdate {
        per_payment_cap,
        period_cap,
        period_seconds: PERIOD_SECONDS,
        expires_at,
    }
}

/// A mandate with 4_000_000 in its vault.
fn funded() -> Fixture {
    let mut f = Fixture::new();
    f.create_mandate();
    f.fund_vault(4_000_000);
    f
}

#[test]
fn update_policy_replaces_the_limits() {
    let mut f = Fixture::new();
    f.create_mandate();

    let ix = f.update_policy_ix(update(1_000_000, 3_000_000, EXPIRES_AT + 1_000));
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);

    let mandate = f.read_mandate();
    assert_eq!(mandate.per_payment_cap, 1_000_000);
    assert_eq!(mandate.period_cap, 3_000_000);
    assert_eq!(mandate.expires_at, EXPIRES_AT + 1_000);
    assert_eq!(
        mandate.allowlist,
        f.policy().allowlist,
        "allowlist untouched"
    );
}

#[test]
fn update_policy_takes_effect_on_the_next_payment() {
    let mut f = funded();
    let recipient = f.allowlist[0];
    let recipient_token = f.token_account_of(&recipient);
    f.set_token_account(recipient_token, &recipient, 0);

    let ix = f.update_policy_ix(update(4_000_000, 4_000_000, EXPIRES_AT));
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);

    // Above the original 2_000_000 per-payment cap, inside the new one.
    let ix = f.spend_ix(3_000_000, recipient_token);
    f.expect_ok(&[ix], &[f.agent.insecure_clone()]);
    assert_eq!(f.token_amount(&recipient_token), 3_000_000);
}

#[test]
fn update_policy_rejects_a_cap_below_what_was_already_spent() {
    let mut f = funded();
    let recipient = f.allowlist[0];
    let recipient_token = f.token_account_of(&recipient);
    f.set_token_account(recipient_token, &recipient, 0);

    let ix = f.spend_ix(2_000_000, recipient_token);
    f.expect_ok(&[ix], &[f.agent.insecure_clone()]);

    let ix = f.update_policy_ix(update(1_000_000, 1_500_000, EXPIRES_AT));
    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "PeriodCapBelowSpent");
    assert_eq!(f.read_mandate().period_cap, PERIOD_CAP, "state untouched");
}

#[test]
fn update_policy_rejects_a_per_payment_cap_above_the_period_cap() {
    let mut f = Fixture::new();
    f.create_mandate();

    let ix = f.update_policy_ix(update(PERIOD_CAP + 1, PERIOD_CAP, EXPIRES_AT));
    f.expect_rejection(
        &[ix],
        &[f.owner.insecure_clone()],
        "PerPaymentCapExceedsPeriodCap",
    );
}

#[test]
fn update_policy_rejects_an_expiry_in_the_past() {
    let mut f = Fixture::new();
    f.create_mandate();

    let ix = f.update_policy_ix(update(PER_PAYMENT_CAP, PERIOD_CAP, -1));
    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "ExpiryInPast");
}

#[test]
fn update_policy_rejects_a_stranger() {
    let mut f = Fixture::new();
    f.create_mandate();

    let stranger = solana_keypair::Keypair::new();
    f.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();

    // The stranger signs as the owner, but the mandate PDA is bound to the real
    // owner, so the seeds the program derives do not match the account.
    let ix = f.update_policy_ix_as(stranger.pubkey(), update(1_000_000, 1_000_000, EXPIRES_AT));
    f.expect_rejection(&[ix], &[stranger], "ConstraintSeeds");
    assert_eq!(f.read_mandate().per_payment_cap, PER_PAYMENT_CAP);
}

#[test]
fn an_added_recipient_can_be_paid() {
    let mut f = funded();

    let recipient = Pubkey::new_unique();
    let token = f.token_account_of(&recipient);
    f.set_token_account(token, &recipient, 0);

    let ix = f.add_recipient_ix(recipient);
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);
    assert!(f.read_mandate().allowlist.contains(&recipient));

    let ix = f.spend_ix(1_000_000, token);
    f.expect_ok(&[ix], &[f.agent.insecure_clone()]);
    assert_eq!(f.token_amount(&token), 1_000_000);
}

#[test]
fn a_removed_recipient_can_no_longer_be_paid() {
    let mut f = funded();
    let former = f.allowlist[0];

    let ix = f.remove_recipient_ix(former);
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);
    assert!(!f.read_mandate().allowlist.contains(&former));

    let token = f.token_account_of(&former);
    f.set_token_account(token, &former, 0);

    let ix = f.spend_ix(1_000_000, token);
    f.expect_rejection(&[ix], &[f.agent.insecure_clone()], "NotAllowlisted");
}

#[test]
fn adding_a_recipient_twice_is_refused() {
    let mut f = Fixture::new();
    f.create_mandate();
    let existing = f.allowlist[0];

    let ix = f.add_recipient_ix(existing);
    f.expect_rejection(
        &[ix],
        &[f.owner.insecure_clone()],
        "RecipientAlreadyAllowed",
    );
}

#[test]
fn removing_an_unknown_recipient_is_refused() {
    let mut f = Fixture::new();
    f.create_mandate();

    let ix = f.remove_recipient_ix(Pubkey::new_unique());
    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "RecipientNotAllowed");
}

#[test]
fn the_allowlist_cannot_grow_past_the_bound() {
    let mut f = Fixture::new();
    let mut policy = f.policy();
    policy.allowlist = (0..mandate::constants::MAX_ALLOWLIST)
        .map(|_| Pubkey::new_unique())
        .collect();
    let ix = f.create_ix(policy);
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);

    let ix = f.add_recipient_ix(Pubkey::new_unique());
    f.expect_rejection(&[ix], &[f.owner.insecure_clone()], "AllowlistTooLong");
}

#[test]
fn recipients_can_be_edited_by_the_owner_only() {
    let mut f = Fixture::new();
    f.create_mandate();

    let stranger = solana_keypair::Keypair::new();
    f.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();

    let ix = f.add_recipient_ix_as(stranger.pubkey(), Pubkey::new_unique());
    f.expect_rejection(&[ix], &[stranger], "ConstraintSeeds");
    assert_eq!(f.read_mandate().allowlist, f.policy().allowlist);
}
