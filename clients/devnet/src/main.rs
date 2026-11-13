//! End to end run of the deployed program against a cluster.
//!
//! ```text
//! MANDATE_RPC_URL=https://api.devnet.solana.com cargo run
//! ```
//!
//! Sets up a mint, a vault and a mandate for a fresh agent key, then exercises
//! every instruction the program exposes and checks the chain agreed: a payment
//! inside the policy moves tokens, one above the cap is refused, the owner edits
//! the policy and the allowlist, and revoke closes the accounts. Keys live under
//! `target/devnet/`, so a rerun reuses them and starts from a fresh mandate.

mod consts;
mod keys;
mod payments;
mod rpc;
mod setup;

use {
    anchor_lang::{prelude::Pubkey, AccountDeserialize},
    keys::Keys,
    mandate::state::Mandate,
    rpc::{explorer, token_amount},
    solana_commitment_config::CommitmentConfig,
    solana_rpc_client::rpc_client::RpcClient,
    solana_signer::Signer,
    std::error::Error,
};

use crate::consts::{OVER_CAP_PAYMENT, PAYMENT, RPC_URL, SECOND_PAYMENT};

fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::var("MANDATE_RPC_URL").unwrap_or_else(|_| RPC_URL.to_string());
    let rpc = RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed());
    let keys = Keys::load()?;

    banner(&rpc, &keys, &url)?;

    setup::fund_agent(&rpc, &keys)?;
    setup::ensure_mint(&rpc, &keys)?;
    setup::ensure_accounts(&rpc, &keys)?;
    setup::ensure_mint_supply(&rpc, &keys)?;
    let mandate = setup::ensure_mandate(&rpc, &keys, expires_at())?;
    setup::fund_vault(&rpc, &keys, &mandate)?;

    let before = State::read(&rpc, &mandate, &keys)?;

    let signature = payments::spend(&rpc, &keys, &mandate, PAYMENT)?;
    println!("\nspend {PAYMENT} allowed: {signature}");
    println!("  {}", explorer(&signature, &url));

    let refusal = payments::spend(&rpc, &keys, &mandate, OVER_CAP_PAYMENT)
        .map_err(|error| error.to_string())
        .expect_err("a payment above the per-payment cap must be refused");
    assert!(
        refusal.contains("CapExceeded"),
        "expected CapExceeded, got:\n{refusal}"
    );
    println!("spend {OVER_CAP_PAYMENT} refused on chain: CapExceeded");

    let after = State::read(&rpc, &mandate, &keys)?;
    report(&before, &after);

    prove_owner_edits(&rpc, &keys, &mandate, &url)?;

    let signature = payments::revoke(&rpc, &keys, &mandate)?;
    println!("\nrevoke: {signature}");
    println!("  {}", explorer(&signature, &url));
    verify_closed(&rpc, &keys, &mandate, after)?;
    Ok(())
}

fn banner(rpc: &RpcClient, keys: &Keys, url: &str) -> Result<(), Box<dyn Error>> {
    println!("rpc      {url}");
    println!("program  {}", mandate::id());
    println!(
        "owner    {} ({} lamports)",
        keys.owner.pubkey(),
        rpc.get_balance(&keys.owner.pubkey())?
    );
    println!("agent    {}", keys.agent.pubkey());
    println!("merchant {}", keys.merchant.pubkey());
    println!("payee    {}", keys.payee.pubkey());
    println!("mint     {}", keys.mint.pubkey());
    Ok(())
}

/// Shows that the owner can change the limits and the allowlist, and that both
/// take effect on the next payment.
fn prove_owner_edits(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    url: &str,
) -> Result<(), Box<dyn Error>> {
    let payee = keys.payee.pubkey();
    setup::ensure_ata(rpc, keys, &payee)?;

    let signature = payments::update_policy(rpc, keys, mandate, expires_at())?;
    println!("\nupdate_policy: {signature}");
    println!("  {}", explorer(&signature, url));

    let signature = payments::add_recipient(rpc, keys, mandate, payee)?;
    println!("add_recipient {payee}: {signature}");
    println!("  {}", explorer(&signature, url));

    let before = token_amount(rpc, &keys.payee_account())?;
    let signature = payments::spend_to(rpc, keys, mandate, SECOND_PAYMENT, keys.payee_account())?;
    println!("spend {SECOND_PAYMENT} to the added recipient: {signature}");
    assert_eq!(
        token_amount(rpc, &keys.payee_account())?,
        before + SECOND_PAYMENT
    );

    let signature = payments::remove_recipient(rpc, keys, mandate, payee)?;
    println!("remove_recipient {payee}: {signature}");
    println!("  {}", explorer(&signature, url));

    let refusal = payments::spend_to(rpc, keys, mandate, SECOND_PAYMENT, keys.payee_account())
        .map_err(|error| error.to_string())
        .expect_err("a removed recipient must not be payable");
    assert!(
        refusal.contains("NotAllowlisted"),
        "expected NotAllowlisted, got:\n{refusal}"
    );
    println!("spend to the removed recipient refused on chain: NotAllowlisted");
    Ok(())
}

/// On-chain state either side of the payment, so a rerun against an existing
/// mandate is judged by the change it made rather than by absolute balances.
struct State {
    spent_in_period: u64,
    vault: u64,
    merchant: u64,
}

impl State {
    fn read(rpc: &RpcClient, mandate: &Pubkey, keys: &Keys) -> Result<Self, Box<dyn Error>> {
        let account = rpc.get_account(mandate)?;
        let mut data: &[u8] = &account.data;
        let state = Mandate::try_deserialize(&mut data)?;
        Ok(Self {
            spent_in_period: state.spent_in_period,
            vault: token_amount(rpc, &keys.vault_account(mandate))?,
            merchant: token_amount(rpc, &keys.merchant_account())?,
        })
    }
}

fn report(before: &State, after: &State) {
    println!("\non chain");
    println!("  mandate spent_in_period {}", after.spent_in_period);
    println!("  vault                     {}", after.vault);
    println!("  merchant                  {}", after.merchant);

    assert_eq!(after.spent_in_period, before.spent_in_period + PAYMENT);
    assert_eq!(after.vault, before.vault - PAYMENT);
    assert_eq!(after.merchant, before.merchant + PAYMENT);
}

fn verify_closed(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    before: State,
) -> Result<(), Box<dyn Error>> {
    assert!(
        rpc.get_account(mandate).is_err(),
        "the mandate account must be closed"
    );
    assert!(
        rpc.get_account(&keys.vault_account(mandate)).is_err(),
        "the vault must be closed"
    );
    let owner = token_amount(rpc, &keys.owner_account())?;
    println!("  mandate and vault closed, owner holds {owner}");
    assert!(
        owner >= before.vault,
        "the vault remainder must reach the owner"
    );
    Ok(())
}

fn expires_at() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the system clock is before the epoch")
        .as_secs();
    let now = i64::try_from(seconds).expect("the clock is within i64 seconds");
    now + crate::consts::MANDATE_LIFETIME_SECONDS
}
