//! End to end run of the deployed program on devnet.
//!
//! ```text
//! cargo run --example devnet_spend
//! ```
//!
//! Creates the mint, the agent key and the merchant key once under
//! `target/devnet/`, so a second run reuses them and only spends again. Prints
//! one payment the policy allows, one it must refuse, and the on-chain state
//! that proves both happened.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token,
        token::{self, spl_token},
    },
    mandate::state::{Mandate, MandatePolicy},
    solana_commitment_config::CommitmentConfig,
    solana_keypair::{read_keypair_file, write_keypair_file, Keypair},
    solana_message::{Message, VersionedMessage},
    solana_program_pack::Pack,
    solana_rpc_client::rpc_client::RpcClient,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    std::{
        error::Error,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    },
};

const RPC_URL: &str = "https://api.devnet.solana.com";
const DECIMALS: u8 = 6;
const MINT_AMOUNT: u64 = 10_000_000;
const VAULT_FUNDING: u64 = 4_000_000;
const PAYMENT: u64 = 1_500_000;
const OVER_CAP_PAYMENT: u64 = 3_000_000;
const PER_PAYMENT_CAP: u64 = 2_000_000;
const PERIOD_CAP: u64 = 5_000_000;
const PERIOD_SECONDS: i64 = 86_400;
const MANDATE_LIFETIME_SECONDS: i64 = 7 * 86_400;
const AGENT_FEE_LAMPORTS: u64 = 20_000_000;

struct Keys {
    owner: Keypair,
    agent: Keypair,
    merchant: Keypair,
    mint: Keypair,
}

fn main() -> Result<(), Box<dyn Error>> {
    let url = std::env::var("MANDATE_RPC_URL").unwrap_or_else(|_| RPC_URL.to_string());
    let rpc = RpcClient::new_with_commitment(url.clone(), CommitmentConfig::confirmed());
    let keys = load_keys()?;

    println!("rpc      {url}");
    println!("program  {}", mandate::id());
    println!("owner    {} ({} lamports)", keys.owner.pubkey(), rpc.get_balance(&keys.owner.pubkey())?);
    println!("agent    {}", keys.agent.pubkey());
    println!("merchant {}", keys.merchant.pubkey());
    println!("mint     {}", keys.mint.pubkey());

    fund_agent(&rpc, &keys)?;
    ensure_mint(&rpc, &keys)?;
    ensure_accounts(&rpc, &keys)?;
    ensure_mint_supply(&rpc, &keys)?;
    let mandate = ensure_mandate(&rpc, &keys)?;
    fund_vault(&rpc, &keys, &mandate)?;

    let before = State::read(&rpc, &mandate, &keys)?;

    let signature = spend(&rpc, &keys, &mandate, PAYMENT)?;
    println!("\nspend {PAYMENT} allowed: {signature}");
    println!("  {}", explorer(&signature, &url));

    let refusal = spend(&rpc, &keys, &mandate, OVER_CAP_PAYMENT)
        .map_err(|error| error.to_string())
        .expect_err("a payment above the per-payment cap must be refused");
    assert!(
        refusal.contains("CapExceeded"),
        "expected CapExceeded, got:\n{refusal}"
    );
    println!("spend {OVER_CAP_PAYMENT} refused on chain: CapExceeded");

    let after = State::read(&rpc, &mandate, &keys)?;
    report(&before, &after);

    let signature = revoke(&rpc, &keys, &mandate)?;
    println!("\nrevoke: {signature}");
    println!("  {}", explorer(&signature, &url));
    verify_closed(&rpc, &keys, &mandate, after)?;
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
    assert!(rpc.get_account(mandate).is_err(), "the mandate account must be closed");
    assert!(
        rpc.get_account(&keys.vault_account(mandate)).is_err(),
        "the vault must be closed"
    );
    let owner = token_amount(rpc, &keys.owner_account())?;
    println!("  mandate and vault closed, owner holds {owner}");
    assert!(owner >= before.vault, "the vault remainder must reach the owner");
    Ok(())
}

/// Explorer links carry the cluster only when the RPC URL names one.
fn explorer(signature: &str, url: &str) -> String {
    let cluster = if url.contains("devnet") {
        "?cluster=devnet"
    } else if url.contains("testnet") {
        "?cluster=testnet"
    } else {
        ""
    };
    format!("https://explorer.solana.com/tx/{signature}{cluster}")
}

fn load_keys() -> Result<Keys, Box<dyn Error>> {
    let home = std::env::var("HOME")?;
    let owner = read_keypair_file(PathBuf::from(home).join(".config/solana/id.json"))?;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/devnet");
    fs::create_dir_all(&dir)?;

    Ok(Keys {
        owner,
        agent: keypair_at(&dir.join("agent.json"))?,
        merchant: keypair_at(&dir.join("merchant.json"))?,
        mint: keypair_at(&dir.join("mint.json"))?,
    })
}

/// Reuses a key on disk so a rerun targets the same accounts.
fn keypair_at(path: &Path) -> Result<Keypair, Box<dyn Error>> {
    if path.exists() {
        return Ok(read_keypair_file(path)?);
    }
    let keypair = Keypair::new();
    write_keypair_file(&keypair, path)?;
    Ok(keypair)
}

fn fund_agent(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
    let balance = rpc.get_balance(&keys.agent.pubkey())?;
    if balance >= AGENT_FEE_LAMPORTS / 2 {
        return Ok(());
    }
    let ix = system_instruction::transfer(
        &keys.owner.pubkey(),
        &keys.agent.pubkey(),
        AGENT_FEE_LAMPORTS,
    );
    send(rpc, &[ix], &[&keys.owner])?;
    println!("funded the agent with {AGENT_FEE_LAMPORTS} lamports for fees");
    Ok(())
}

fn ensure_mint(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
    let mint = keys.mint.pubkey();
    if rpc.get_account(&mint).is_ok() {
        return Ok(());
    }
    let rent = rpc.get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?;
    let create = system_instruction::create_account(
        &keys.owner.pubkey(),
        &mint,
        rent,
        spl_token::state::Mint::LEN as u64,
        &token::ID,
    );
    let init = spl_token::instruction::initialize_mint2(&token::ID, &mint, &keys.owner.pubkey(), None, DECIMALS)?;
    send(rpc, &[create, init], &[&keys.owner, &keys.mint])?;
    println!("created mint {mint} with {DECIMALS} decimals");
    Ok(())
}

fn ensure_accounts(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
    let payer = keys.owner.pubkey();
    let mint = keys.mint.pubkey();
    let mandate = mandate_address(&payer, &keys.agent.pubkey());

    let mut missing = Vec::new();
    for authority in [payer, keys.merchant.pubkey(), mandate] {
        let address = associated_token_address(&authority, &mint);
        if rpc.get_account(&address).is_err() {
            missing.push(
                associated_token::spl_associated_token_account::instruction::create_associated_token_account_idempotent(
                    &payer, &authority, &mint, &token::ID,
                ),
            );
        }
    }

    if missing.is_empty() {
        return Ok(());
    }
    send(rpc, &missing, &[&keys.owner])?;
    println!("created the owner, merchant and vault token accounts");
    Ok(())
}

fn ensure_mint_supply(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
    let owner_account = keys.owner_account();
    if token_amount(rpc, &owner_account)? >= MINT_AMOUNT {
        return Ok(());
    }
    let ix = spl_token::instruction::mint_to(
        &token::ID,
        &keys.mint.pubkey(),
        &owner_account,
        &keys.owner.pubkey(),
        &[],
        MINT_AMOUNT,
    )?;
    send(rpc, &[ix], &[&keys.owner])?;
    println!("minted {MINT_AMOUNT} units to the owner");
    Ok(())
}

fn ensure_mandate(
    rpc: &RpcClient,
    keys: &Keys,
) -> Result<Pubkey, Box<dyn Error>> {
    let owner = keys.owner.pubkey();
    let mandate = mandate_address(&owner, &keys.agent.pubkey());
    if rpc.get_account(&mandate).is_ok() {
        println!("mandate  {mandate} already exists");
        return Ok(mandate);
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let policy = MandatePolicy {
        mint: keys.mint.pubkey(),
        per_payment_cap: PER_PAYMENT_CAP,
        period_cap: PERIOD_CAP,
        period_seconds: PERIOD_SECONDS,
        expires_at: now + MANDATE_LIFETIME_SECONDS,
        allowlist: vec![keys.merchant.pubkey()],
    };
    let ix = Instruction::new_with_bytes(
        mandate::id(),
        &mandate::instruction::CreateMandate {
            agent: keys.agent.pubkey(),
            policy,
        }
        .data(),
        mandate::accounts::CreateMandate {
            owner,
            mandate,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(rpc, &[ix], &[&keys.owner])?;
    println!("mandate  {mandate} created for the agent");
    Ok(mandate)
}

fn fund_vault(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
) -> Result<(), Box<dyn Error>> {
    let vault = keys.vault_account(mandate);
    let held = token_amount(rpc, &vault)?;
    if held >= VAULT_FUNDING / 2 {
        return Ok(());
    }
    let ix = spl_token::instruction::transfer(
        &token::ID,
        &keys.owner_account(),
        &vault,
        &keys.owner.pubkey(),
        &[],
        VAULT_FUNDING,
    )?;
    send(rpc, &[ix], &[&keys.owner])?;
    println!("vault    {vault} funded with {VAULT_FUNDING} units");
    Ok(())
}

fn spend(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    amount: u64,
) -> Result<String, Box<dyn Error>> {
    let ix = Instruction::new_with_bytes(
        mandate::id(),
        &mandate::instruction::Spend { amount }.data(),
        mandate::accounts::Spend {
            agent: keys.agent.pubkey(),
            owner: keys.owner.pubkey(),
            mandate: *mandate,
            vault: keys.vault_account(mandate),
            recipient_token: keys.merchant_account(),
            token_program: token::ID,
        }
        .to_account_metas(None),
    );
    send(rpc, &[ix], &[&keys.agent])
}

fn revoke(rpc: &RpcClient, keys: &Keys, mandate: &Pubkey) -> Result<String, Box<dyn Error>> {
    let ix = Instruction::new_with_bytes(
        mandate::id(),
        &mandate::instruction::Revoke {}.data(),
        mandate::accounts::Revoke {
            owner: keys.owner.pubkey(),
            agent: keys.agent.pubkey(),
            mandate: *mandate,
            vault: keys.vault_account(mandate),
            owner_token: keys.owner_account(),
            token_program: token::ID,
        }
        .to_account_metas(None),
    );
    send(rpc, &[ix], &[&keys.owner])
}

impl Keys {
    fn owner_account(&self) -> Pubkey {
        associated_token_address(&self.owner.pubkey(), &self.mint.pubkey())
    }

    fn merchant_account(&self) -> Pubkey {
        associated_token_address(&self.merchant.pubkey(), &self.mint.pubkey())
    }

    fn vault_account(&self, mandate: &Pubkey) -> Pubkey {
        associated_token_address(mandate, &self.mint.pubkey())
    }
}

fn associated_token_address(authority: &Pubkey, mint: &Pubkey) -> Pubkey {
    associated_token::get_associated_token_address(authority, mint)
}

fn mandate_address(owner: &Pubkey, agent: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[mandate::constants::MANDATE_SEED, owner.as_ref(), agent.as_ref()],
        &mandate::id(),
    )
    .0
}

fn token_amount(rpc: &RpcClient, address: &Pubkey) -> Result<u64, Box<dyn Error>> {
    let balance = rpc.get_token_account_balance(address)?;
    Ok(balance.amount.parse()?)
}

fn send(
    rpc: &RpcClient,
    ixs: &[Instruction],
    signers: &[&Keypair],
) -> Result<String, Box<dyn Error>> {
    let payer = signers.first().expect("a transaction needs a payer").pubkey();
    let blockhash = rpc.get_latest_blockhash()?;
    let message = Message::new_with_blockhash(ixs, Some(&payer), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(message), signers)?;
    let signature = rpc.send_and_confirm_transaction(&tx).map_err(|error| error.to_string())?;
    Ok(signature.to_string())
}
