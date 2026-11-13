//! Owner-signed shutdown. Returns the vault remainder and reclaims rent.

use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Token, TokenAccount, Transfer};

use crate::{constants::MANDATE_SEED, state::Mandate};

#[derive(Accounts)]
pub struct Revoke<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    /// CHECK: only re-derives the mandate PDA, bound to the account by `has_one`.
    pub agent: UncheckedAccount<'info>,
    #[account(
        mut,
        close = owner,
        seeds = [MANDATE_SEED, owner.key().as_ref(), agent.key().as_ref()],
        bump,
        has_one = owner,
        has_one = agent
    )]
    pub mandate: Account<'info, Mandate>,
    #[account(mut, token::mint = mandate.mint, token::authority = mandate)]
    pub vault: Account<'info, TokenAccount>,
    #[account(mut, token::mint = mandate.mint, token::authority = owner)]
    pub owner_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_revoke(ctx: Context<Revoke>) -> Result<()> {
    let remainder = ctx.accounts.vault.amount;
    let owner_key = ctx.accounts.mandate.owner;
    let agent_key = ctx.accounts.mandate.agent;
    let bump = [ctx.bumps.mandate];
    let signer_seeds: &[&[u8]] = &[MANDATE_SEED, owner_key.as_ref(), agent_key.as_ref(), &bump];

    if remainder > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.owner_token.to_account_info(),
                    authority: ctx.accounts.mandate.to_account_info(),
                },
                &[signer_seeds],
            ),
            remainder,
        )?;
    }

    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.owner.to_account_info(),
            authority: ctx.accounts.mandate.to_account_info(),
        },
        &[signer_seeds],
    ))?;

    Ok(())
}
