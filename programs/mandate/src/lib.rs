//! On-chain spend limits for an agent's payments.
//!
//! An owner binds a policy to an agent key in a PDA. A later payment is judged
//! against that policy by this program, not by the server that asked for it.

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

#[cfg(test)]
mod tests;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("9izrkU3aCb3KxAcfzErm91GCSnyFEsqTkQHgKWb4hTYN");

#[program]
pub mod mandate {
    use super::*;

    /// Creates the mandate PDA for `agent` and stores `policy` in it.
    pub fn create_mandate(
        ctx: Context<CreateMandate>,
        agent: Pubkey,
        policy: MandatePolicy,
    ) -> Result<()> {
        instructions::create_mandate::handle_create_mandate(ctx, agent, policy)
    }

    /// Pays `amount` to an allowlisted recipient if the policy allows it.
    pub fn spend(ctx: Context<Spend>, amount: u64) -> Result<()> {
        instructions::spend::handle_spend(ctx, amount)
    }

    /// Returns the vault remainder to the owner and closes both accounts.
    pub fn revoke(ctx: Context<Revoke>) -> Result<()> {
        instructions::revoke::handle_revoke(ctx)
    }

    /// Replaces the numeric limits of an existing mandate.
    pub fn update_policy(ctx: Context<EditMandate>, update: PolicyUpdate) -> Result<()> {
        instructions::owner::handle_update_policy(ctx, update)
    }

    /// Adds one recipient to the allowlist, bounded at eight.
    pub fn add_recipient(ctx: Context<EditMandate>, recipient: Pubkey) -> Result<()> {
        instructions::owner::handle_add_recipient(ctx, recipient)
    }

    /// Removes one recipient from the allowlist.
    pub fn remove_recipient(ctx: Context<EditMandate>, recipient: Pubkey) -> Result<()> {
        instructions::owner::handle_remove_recipient(ctx, recipient)
    }
}
