//! Owner-side edits to a mandate that already exists.
//!
//! The three instructions here differ only in which fields they write, so they
//! share one accounts struct. All of them need the owner's signature and the
//! canonical mandate PDA: an agent cannot widen its own limits and no one else
//! can widen them for it.

use anchor_lang::prelude::*;

use crate::{
    constants::MANDATE_SEED,
    error::ErrorCode,
    state::{Mandate, PolicyUpdate, PolicyUpdated, RecipientAdded, RecipientRemoved},
};

#[derive(Accounts)]
pub struct EditMandate<'info> {
    pub owner: Signer<'info>,
    /// CHECK: only re-derives the mandate PDA, bound to the account by `has_one`.
    pub agent: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [MANDATE_SEED, owner.key().as_ref(), agent.key().as_ref()],
        bump,
        has_one = owner,
        has_one = agent
    )]
    pub mandate: Account<'info, Mandate>,
}

/// Applies an owner-supplied replacement of the numeric limits.
pub fn handle_update_policy(ctx: Context<EditMandate>, update: PolicyUpdate) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let mandate = &mut ctx.accounts.mandate;
    mandate.apply_update(&update, now)?;

    emit!(PolicyUpdated {
        owner: mandate.owner,
        agent: mandate.agent,
        per_payment_cap: mandate.per_payment_cap,
        period_cap: mandate.period_cap,
        period_seconds: mandate.period_seconds,
        expires_at: mandate.expires_at,
    });
    Ok(())
}

/// Allows one more recipient to be paid.
pub fn handle_add_recipient(ctx: Context<EditMandate>, recipient: Pubkey) -> Result<()> {
    let mandate = &mut ctx.accounts.mandate;
    mandate.add_recipient(recipient)?;

    emit!(RecipientAdded {
        owner: mandate.owner,
        agent: mandate.agent,
        recipient,
        allowlist_len: allowlist_len(mandate)?,
    });
    Ok(())
}

/// Stops one recipient from being paid.
pub fn handle_remove_recipient(ctx: Context<EditMandate>, recipient: Pubkey) -> Result<()> {
    let mandate = &mut ctx.accounts.mandate;
    mandate.remove_recipient(recipient)?;

    emit!(RecipientRemoved {
        owner: mandate.owner,
        agent: mandate.agent,
        recipient,
        allowlist_len: allowlist_len(mandate)?,
    });
    Ok(())
}

/// The allowlist is bounded at eight, so the length always fits in a byte.
fn allowlist_len(mandate: &Mandate) -> Result<u8> {
    u8::try_from(mandate.allowlist.len()).map_err(|_| error!(ErrorCode::AllowlistTooLong))
}
