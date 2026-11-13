//! `revoke` hands the remainder back to the owner and reclaims the accounts.

mod support;

use support::*;

#[test]
fn revoke_returns_the_remainder_and_closes_the_mandate() {
    let mut f = Fixture::new();
    f.create_mandate();

    let vault = f.vault_address();
    f.set_token_account(vault, &f.mandate_address(), 3_000_000);
    let owner_token = f.token_account_of(&f.owner.pubkey());
    f.set_token_account(owner_token, &f.owner.pubkey(), 500_000);

    let ix = f.revoke_ix();
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);

    assert_eq!(f.token_amount(&owner_token), 3_500_000);
    assert!(f.svm.get_account(&vault).is_none(), "vault must be closed");
    assert!(
        f.svm.get_account(&f.mandate_address()).is_none(),
        "mandate must be closed"
    );
}

#[test]
fn revoke_closes_an_empty_vault() {
    let mut f = Fixture::new();
    f.create_mandate();

    let vault = f.vault_address();
    f.set_token_account(vault, &f.mandate_address(), 0);
    let owner_token = f.token_account_of(&f.owner.pubkey());
    f.set_token_account(owner_token, &f.owner.pubkey(), 0);

    let ix = f.revoke_ix();
    f.expect_ok(&[ix], &[f.owner.insecure_clone()]);

    assert!(f.svm.get_account(&vault).is_none(), "vault must be closed");
    assert_eq!(f.token_amount(&owner_token), 0);
}

#[test]
fn rejects_a_revoke_attempted_by_a_stranger() {
    let mut f = Fixture::new();
    f.create_mandate();

    let vault = f.vault_address();
    f.set_token_account(vault, &f.mandate_address(), 3_000_000);

    let stranger = solana_keypair::Keypair::new();
    f.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let stranger_token = f.token_account_of(&stranger.pubkey());
    f.set_token_account(stranger_token, &stranger.pubkey(), 0);

    // The stranger signs, but the mandate PDA is bound to the real owner, so
    // the seeds the program derives do not match the account it was handed.
    let ix = f.revoke_ix_as(stranger.pubkey());
    f.expect_rejection(&[ix], &[stranger], "ConstraintSeeds");

    assert_eq!(f.token_amount(&vault), 3_000_000);
    assert_eq!(f.token_amount(&stranger_token), 0);
    assert!(f.svm.get_account(&f.mandate_address()).is_some());
}
