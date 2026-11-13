//! Locks a spending policy into a PDA derived from the owner and the agent.

use anchor_lang::prelude::*;

use crate::{
    constants::MANDATE_SEED,
    error::ErrorCode,
    state::{Mandate, MandateCreated, MandatePolicy},
};

#[derive(Accounts)]
#[instruction(agent: Pubkey)]
pub struct CreateMandate<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(
        init,
        payer = owner,
        space = 8 + Mandate::INIT_SPACE,
        seeds = [MANDATE_SEED, owner.key().as_ref(), agent.as_ref()],
        bump
    )]
    pub mandate: Account<'info, Mandate>,
    pub system_program: Program<'info, System>,
}

/// Validates `policy` and writes the new mandate account.
pub fn handle_create_mandate(
    ctx: Context<CreateMandate>,
    agent: Pubkey,
    policy: MandatePolicy,
) -> Result<()> {
    let owner = ctx.accounts.owner.key();
    require_keys_neq!(owner, agent, ErrorCode::AgentIsOwner);

    let now = Clock::get()?.unix_timestamp;
    policy.validate(now)?;

    ctx.accounts
        .mandate
        .set_inner(Mandate::from_policy(owner, agent, &policy, now));

    let mandate = &ctx.accounts.mandate;
    emit!(MandateCreated {
        owner: mandate.owner,
        agent: mandate.agent,
        mint: mandate.mint,
        per_payment_cap: mandate.per_payment_cap,
        period_cap: mandate.period_cap,
        period_seconds: mandate.period_seconds,
        expires_at: mandate.expires_at,
    });

    Ok(())
}
