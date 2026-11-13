//! `spend` is judged against the mandate, not against the caller's intent.

mod support;

use {anchor_lang::prelude::Pubkey, support::*};

/// Creates a mandate with a vault holding `vault_amount`, and a token account
/// for the first allowlisted recipient.
fn funded(vault_amount: u64) -> (Fixture, Pubkey, Pubkey) {
    let mut f = Fixture::new();
    f.create_mandate();

    let vault = f.vault_address();
    f.set_token_account(vault, &f.mandate_address(), vault_amount);

    let recipient = f.allowlist[0];
    let recipient_token = f.token_account_of(&recipient);
    f.set_token_account(recipient_token, &recipient, 0);

    (f, vault, recipient_token)
}

#[test]
fn spend_moves_tokens_and_records_the_payment() {
    let (mut f, vault, recipient_token) = funded(4_000_000);

    let ix = f.spend_ix(1_500_000, recipient_token);
    f.expect_ok(&[ix], &[f.agent.insecure_clone()]);

    assert_eq!(f.token_amount(&vault), 2_500_000);
    assert_eq!(f.token_amount(&recipient_token), 1_500_000);
    assert_eq!(f.read_mandate().spent_in_period, 1_500_000);
}

#[test]
fn rejects_a_payment_above_the_per_payment_cap() {
    let (mut f, vault, recipient_token) = funded(4_000_000);

    let ix = f.spend_ix(PER_PAYMENT_CAP + 1, recipient_token);
    f.expect_rejection(&[ix], &[f.agent.insecure_clone()], "CapExceeded");
    assert_eq!(f.token_amount(&vault), 4_000_000);
    assert_eq!(f.read_mandate().spent_in_period, 0);
}

#[test]
fn rejects_a_payment_that_would_exhaust_the_period() {
    let (mut f, _, recipient_token) = funded(4_000_000);

    for _ in 0..2 {
        let ix = f.spend_ix(PER_PAYMENT_CAP, recipient_token);
        f.expect_ok(&[ix], &[f.agent.insecure_clone()]);
    }
    assert_eq!(f.read_mandate().spent_in_period, 4_000_000);

    let ix = f.spend_ix(2_000_000, recipient_token);
    f.expect_rejection(&[ix], &[f.agent.insecure_clone()], "PeriodExhausted");
    assert_eq!(f.read_mandate().spent_in_period, 4_000_000);
}

#[test]
fn spends_again_once_the_period_elapses() {
    // Three per-payment caps have to fit in the vault.
    let (mut f, _, recipient_token) = funded(3 * PER_PAYMENT_CAP);

    for _ in 0..2 {
        let ix = f.spend_ix(PER_PAYMENT_CAP, recipient_token);
        f.expect_ok(&[ix], &[f.agent.insecure_clone()]);
    }

    f.set_clock(PERIOD_SECONDS + 1);
    let ix = f.spend_ix(PER_PAYMENT_CAP, recipient_token);
    f.expect_ok(&[ix], &[f.agent.insecure_clone()]);

    let mandate = f.read_mandate();
    assert_eq!(mandate.spent_in_period, PER_PAYMENT_CAP);
    assert_eq!(mandate.period_start, PERIOD_SECONDS + 1);
}

#[test]
fn rejects_a_recipient_not_on_the_allowlist() {
    let (mut f, _, _) = funded(4_000_000);

    let stranger = Pubkey::new_unique();
    let stranger_token = f.token_account_of(&stranger);
    f.set_token_account(stranger_token, &stranger, 0);

    let ix = f.spend_ix(1_000_000, stranger_token);
    f.expect_rejection(&[ix], &[f.agent.insecure_clone()], "NotAllowlisted");
    assert_eq!(f.token_amount(&stranger_token), 0);
}

#[test]
fn rejects_after_the_mandate_expires() {
    let (mut f, vault, recipient_token) = funded(4_000_000);

    f.set_clock(EXPIRES_AT + 1);
    let ix = f.spend_ix(1_000_000, recipient_token);
    f.expect_rejection(&[ix], &[f.agent.insecure_clone()], "Expired");
    assert_eq!(f.token_amount(&vault), 4_000_000);
}

#[test]
fn rejects_a_spend_signed_by_anyone_but_the_agent() {
    let (mut f, _, recipient_token) = funded(4_000_000);

    let impostor = solana_keypair::Keypair::new();
    f.svm.airdrop(&impostor.pubkey(), 1_000_000_000).unwrap();

    let ix = f.spend_ix_as(impostor.pubkey(), 1_000_000, recipient_token);
    f.expect_rejection(&[ix], &[impostor], "ConstraintSeeds");
    assert_eq!(f.token_amount(&recipient_token), 0);
}
