//! Agent-signed payment. The chain, not the caller, decides if it fits.

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::{
    constants::MANDATE_SEED,
    error::ErrorCode,
    state::{Mandate, Spent},
};

#[derive(Accounts)]
pub struct Spend<'info> {
    pub agent: Signer<'info>,
    /// CHECK: only re-derives the mandate PDA, bound to the account by `has_one`.
    pub owner: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [MANDATE_SEED, owner.key().as_ref(), agent.key().as_ref()],
        bump,
        has_one = owner,
        has_one = agent
    )]
    pub mandate: Account<'info, Mandate>,
    #[account(mut, token::mint = mandate.mint, token::authority = mandate)]
    pub vault: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mandate.mint)]
    pub recipient_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_spend(ctx: Context<Spend>, amount: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let recipient = ctx.accounts.recipient_token.owner;

    {
        let mandate = &mut ctx.accounts.mandate;
        require!(mandate.allows(&recipient), ErrorCode::NotAllowlisted);
        mandate.charge(amount, now)?;
    }

    let owner = ctx.accounts.mandate.owner;
    let agent = ctx.accounts.mandate.agent;
    let bump = [ctx.bumps.mandate];
    let signer_seeds: &[&[u8]] = &[MANDATE_SEED, owner.as_ref(), agent.as_ref(), &bump];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.recipient_token.to_account_info(),
                authority: ctx.accounts.mandate.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount,
    )?;

    emit!(Spent {
        owner,
        agent,
        recipient,
        amount,
        spent_in_period: ctx.accounts.mandate.spent_in_period,
    });

    Ok(())
}
