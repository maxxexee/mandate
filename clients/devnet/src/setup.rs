//! Everything that has to exist before a payment can be attempted: the mint,
//! the token accounts, the mandate and its vault.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token,
        token::{self, spl_token},
    },
    mandate::state::MandatePolicy,
    solana_program_pack::Pack,
    solana_rpc_client::rpc_client::RpcClient,
    solana_signer::Signer,
    std::error::Error,
};

use crate::{
    consts::*,
    keys::{associated_token_address, Keys},
    rpc::{send, token_amount},
};

pub(crate) fn fund_agent(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
    if rpc.get_balance(&keys.agent.pubkey())? >= AGENT_FEE_LAMPORTS / 2 {
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

pub(crate) fn ensure_mint(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
    let mint = keys.mint.pubkey();
    if rpc.get_account(&mint).is_ok() {
        return Ok(());
    }
    let rent = rpc.get_minimum_balance_for_rent_exemption(spl_token::state::Mint::LEN)?;
    let create = system_instruction::create_account(
        &keys.owner.pubkey(),
        &mint,
        rent,
        u64::try_from(spl_token::state::Mint::LEN)?,
        &token::ID,
    );
    let init = spl_token::instruction::initialize_mint2(
        &token::ID,
        &mint,
        &keys.owner.pubkey(),
        None,
        DECIMALS,
    )?;
    send(rpc, &[create, init], &[&keys.owner, &keys.mint])?;
    println!("created mint {mint} with {DECIMALS} decimals");
    Ok(())
}

pub(crate) fn ensure_accounts(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
    let authorities = [
        keys.owner.pubkey(),
        keys.merchant.pubkey(),
        keys.payee.pubkey(),
        keys.mandate_address(),
    ];

    let mut created = 0;
    for authority in authorities {
        if ensure_ata(rpc, keys, &authority)? {
            created += 1;
        }
    }
    if created > 0 {
        println!("created {created} token accounts");
    }
    Ok(())
}

/// Creates the associated token account for `authority` if it is missing and
/// reports whether it had to. One transaction per account keeps a rerun quiet.
pub(crate) fn ensure_ata(
    rpc: &RpcClient,
    keys: &Keys,
    authority: &Pubkey,
) -> Result<bool, Box<dyn Error>> {
    let address = associated_token_address(authority, &keys.mint.pubkey());
    if rpc.get_account(&address).is_ok() {
        return Ok(false);
    }
    let ix = associated_token::spl_associated_token_account::instruction::
        create_associated_token_account_idempotent(
            &keys.owner.pubkey(),
            authority,
            &keys.mint.pubkey(),
            &token::ID,
        );
    send(rpc, &[ix], &[&keys.owner])?;
    Ok(true)
}

pub(crate) fn ensure_mint_supply(rpc: &RpcClient, keys: &Keys) -> Result<(), Box<dyn Error>> {
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

pub(crate) fn ensure_mandate(
    rpc: &RpcClient,
    keys: &Keys,
    expires_at: i64,
) -> Result<Pubkey, Box<dyn Error>> {
    let mandate = keys.mandate_address();
    if rpc.get_account(&mandate).is_ok() {
        println!("mandate  {mandate} already exists");
        return Ok(mandate);
    }

    let policy = MandatePolicy {
        mint: keys.mint.pubkey(),
        per_payment_cap: PER_PAYMENT_CAP,
        period_cap: PERIOD_CAP,
        period_seconds: PERIOD_SECONDS,
        expires_at,
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
            owner: keys.owner.pubkey(),
            mandate,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(rpc, &[ix], &[&keys.owner])?;
    println!("mandate  {mandate} created for the agent");
    Ok(mandate)
}

pub(crate) fn fund_vault(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
) -> Result<(), Box<dyn Error>> {
    let vault = keys.vault_account(mandate);
    if token_amount(rpc, &vault)? >= VAULT_FUNDING / 2 {
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
