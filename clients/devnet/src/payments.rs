//! Every instruction the program exposes, as transaction builders.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::instruction::{AccountMeta, Instruction},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token,
    mandate::state::PolicyUpdate,
    solana_rpc_client::rpc_client::RpcClient,
    solana_signer::Signer,
    std::error::Error,
};

use crate::{consts::*, keys::Keys, rpc::send};

pub(crate) fn spend(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    amount: u64,
) -> Result<String, Box<dyn Error>> {
    spend_to(rpc, keys, mandate, amount, keys.merchant_account())
}

pub(crate) fn spend_to(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    amount: u64,
    recipient_token: Pubkey,
) -> Result<String, Box<dyn Error>> {
    let ix = Instruction::new_with_bytes(
        mandate::id(),
        &mandate::instruction::Spend { amount }.data(),
        mandate::accounts::Spend {
            agent: keys.agent.pubkey(),
            owner: keys.owner.pubkey(),
            mandate: *mandate,
            vault: keys.vault_account(mandate),
            recipient_token,
            token_program: token::ID,
        }
        .to_account_metas(None),
    );
    send(rpc, &[ix], &[&keys.agent])
}

pub(crate) fn revoke(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
) -> Result<String, Box<dyn Error>> {
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

pub(crate) fn update_policy(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    expires_at: i64,
) -> Result<String, Box<dyn Error>> {
    let ix = Instruction::new_with_bytes(
        mandate::id(),
        &mandate::instruction::UpdatePolicy {
            update: PolicyUpdate {
                per_payment_cap: PER_PAYMENT_CAP,
                period_cap: PERIOD_CAP,
                period_seconds: PERIOD_SECONDS,
                expires_at,
            },
        }
        .data(),
        edit_accounts(keys, mandate),
    );
    send(rpc, &[ix], &[&keys.owner])
}

pub(crate) fn add_recipient(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    recipient: Pubkey,
) -> Result<String, Box<dyn Error>> {
    let ix = Instruction::new_with_bytes(
        mandate::id(),
        &mandate::instruction::AddRecipient { recipient }.data(),
        edit_accounts(keys, mandate),
    );
    send(rpc, &[ix], &[&keys.owner])
}

pub(crate) fn remove_recipient(
    rpc: &RpcClient,
    keys: &Keys,
    mandate: &Pubkey,
    recipient: Pubkey,
) -> Result<String, Box<dyn Error>> {
    let ix = Instruction::new_with_bytes(
        mandate::id(),
        &mandate::instruction::RemoveRecipient { recipient }.data(),
        edit_accounts(keys, mandate),
    );
    send(rpc, &[ix], &[&keys.owner])
}

fn edit_accounts(keys: &Keys, mandate: &Pubkey) -> Vec<AccountMeta> {
    mandate::accounts::EditMandate {
        owner: keys.owner.pubkey(),
        agent: keys.agent.pubkey(),
        mandate: *mandate,
    }
    .to_account_metas(None)
}
