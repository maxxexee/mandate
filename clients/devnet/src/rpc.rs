//! The few RPC interactions the flows share.

use {
    anchor_lang::{prelude::Pubkey, solana_program::instruction::Instruction},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_rpc_client::rpc_client::RpcClient,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    std::error::Error,
};

/// Signs and confirms `ixs`, paying from the first signer.
pub(crate) fn send(
    rpc: &RpcClient,
    ixs: &[Instruction],
    signers: &[&Keypair],
) -> Result<String, Box<dyn Error>> {
    let payer = signers
        .first()
        .expect("a transaction needs a payer")
        .pubkey();
    let blockhash = rpc.get_latest_blockhash()?;
    let message = Message::new_with_blockhash(ixs, Some(&payer), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(message), signers)?;
    let signature = rpc
        .send_and_confirm_transaction(&tx)
        .map_err(|error| error.to_string())?;
    Ok(signature.to_string())
}

pub(crate) fn token_amount(rpc: &RpcClient, address: &Pubkey) -> Result<u64, Box<dyn Error>> {
    let balance = rpc.get_token_account_balance(address)?;
    Ok(balance.amount.parse()?)
}

/// Explorer links carry the cluster only when the RPC URL names one.
pub(crate) fn explorer(signature: &str, url: &str) -> String {
    let cluster = if url.contains("devnet") {
        "?cluster=devnet"
    } else if url.contains("testnet") {
        "?cluster=testnet"
    } else {
        ""
    };
    format!("https://explorer.solana.com/tx/{signature}{cluster}")
}
