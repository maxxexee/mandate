use anchor_lang::prelude::*;

use crate::error::ErrorCode;

/// Spending policy supplied by the owner when a mandate is created.
///
/// Amounts are in the mint's smallest unit, so callers must apply the mint's
/// decimals before building a policy. `period_seconds` and `expires_at` are
/// Unix seconds.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct MandatePolicy {
    pub mint: Pubkey,
    pub per_payment_cap: u64,
    pub period_cap: u64,
    pub period_seconds: i64,
    pub expires_at: i64,
    pub allowlist: Vec<Pubkey>,
}

impl MandatePolicy {
    /// Rejects policies that no later `spend` could ever satisfy.
    ///
    /// Checked once at creation so `spend` never has to re-derive whether the
    /// policy is coherent, only whether a payment fits inside it.
    pub fn validate(&self, now: i64) -> Result<()> {
        require!(self.per_payment_cap > 0, ErrorCode::ZeroPerPaymentCap);
        require!(
            self.per_payment_cap <= self.period_cap,
            ErrorCode::PerPaymentCapExceedsPeriodCap
        );
        require!(self.period_seconds > 0, ErrorCode::ZeroPeriod);
        require!(self.expires_at > now, ErrorCode::ExpiryInPast);
        require!(!self.allowlist.is_empty(), ErrorCode::EmptyAllowlist);
        require!(
            self.allowlist.len() <= crate::constants::MAX_ALLOWLIST,
            ErrorCode::AllowlistTooLong
        );
        Ok(())
    }
}

/// Owner-supplied replacement for the numeric limits of a live mandate.
///
/// The allowlist is not part of it: recipients are edited one at a time by their
/// own instructions, so an owner cannot drop the whole list by resending a stale
/// copy. A struct rather than four arguments also leaves room to add a limit
/// without changing the instruction signature.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct PolicyUpdate {
    pub per_payment_cap: u64,
    pub period_cap: u64,
    pub period_seconds: i64,
    pub expires_at: i64,
}

/// Spending authority an owner has bound to one agent for one mint.
///
/// The bump is not stored. It is re-derived from the seeds on every spend,
/// which keeps a stale bump from ever being trusted.
#[account]
#[derive(InitSpace)]
pub struct Mandate {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub mint: Pubkey,
    pub per_payment_cap: u64,
    pub period_cap: u64,
    pub period_seconds: i64,
    pub period_start: i64,
    pub spent_in_period: u64,
    pub expires_at: i64,
    #[max_len(8)]
    pub allowlist: Vec<Pubkey>,
}

impl Mandate {
    /// Builds the account state for a policy that has already been validated.
    ///
    /// The first period starts at creation, so the agent cannot spend a full
    /// period's cap twice by racing the first `spend`.
    pub fn from_policy(owner: Pubkey, agent: Pubkey, policy: &MandatePolicy, now: i64) -> Self {
        Self {
            owner,
            agent,
            mint: policy.mint,
            per_payment_cap: policy.per_payment_cap,
            period_cap: policy.period_cap,
            period_seconds: policy.period_seconds,
            period_start: now,
            spent_in_period: 0,
            expires_at: policy.expires_at,
            allowlist: policy.allowlist.clone(),
        }
    }

    /// Whether `recipient` may be paid under this mandate.
    pub fn allows(&self, recipient: &Pubkey) -> bool {
        self.allowlist.contains(recipient)
    }

    /// Charges `amount` against the policy and records it.
    ///
    /// The period is rolled over lazily here rather than by a separate crank, so
    /// a mandate with no spends costs nothing to keep. Every field write happens
    /// only after all checks pass, so a rejected spend leaves the account as it
    /// was even though the transaction already borrowed it mutably.
    pub fn charge(&mut self, amount: u64, now: i64) -> Result<()> {
        require!(now < self.expires_at, ErrorCode::Expired);
        require!(amount > 0, ErrorCode::ZeroAmount);
        require!(amount <= self.per_payment_cap, ErrorCode::CapExceeded);

        if now >= self.period_start.saturating_add(self.period_seconds) {
            self.period_start = now;
            self.spent_in_period = 0;
        }

        let spent = self
            .spent_in_period
            .checked_add(amount)
            .ok_or(ErrorCode::PeriodExhausted)?;
        require!(spent <= self.period_cap, ErrorCode::PeriodExhausted);

        self.spent_in_period = spent;
        Ok(())
    }

    /// Replaces the numeric limits, refusing a combination the mandate could
    /// never satisfy.
    ///
    /// A new `period_cap` below what this period already spent is rejected, so a
    /// mandate can never sit in a state that looks over budget. The running
    /// period keeps its start, which means shortening `period_seconds` takes
    /// effect at the boundary already in flight.
    pub fn apply_update(&mut self, update: &PolicyUpdate, now: i64) -> Result<()> {
        require!(update.per_payment_cap > 0, ErrorCode::ZeroPerPaymentCap);
        require!(
            update.per_payment_cap <= update.period_cap,
            ErrorCode::PerPaymentCapExceedsPeriodCap
        );
        require!(update.period_seconds > 0, ErrorCode::ZeroPeriod);
        require!(update.expires_at > now, ErrorCode::ExpiryInPast);
        require!(
            update.period_cap >= self.spent_in_period,
            ErrorCode::PeriodCapBelowSpent
        );

        self.per_payment_cap = update.per_payment_cap;
        self.period_cap = update.period_cap;
        self.period_seconds = update.period_seconds;
        self.expires_at = update.expires_at;
        Ok(())
    }

    /// Adds a recipient, bounded by the account's fixed allowlist size.
    pub fn add_recipient(&mut self, recipient: Pubkey) -> Result<()> {
        require!(!self.allows(&recipient), ErrorCode::RecipientAlreadyAllowed);
        require!(
            self.allowlist.len() < crate::constants::MAX_ALLOWLIST,
            ErrorCode::AllowlistTooLong
        );
        self.allowlist.push(recipient);
        Ok(())
    }

    /// Removes a recipient, keeping the order of the rest.
    pub fn remove_recipient(&mut self, recipient: Pubkey) -> Result<()> {
        let index = self
            .allowlist
            .iter()
            .position(|allowed| *allowed == recipient)
            .ok_or(error!(ErrorCode::RecipientNotAllowed))?;
        self.allowlist.remove(index);
        Ok(())
    }
}

#[event]
pub struct MandateCreated {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub mint: Pubkey,
    pub per_payment_cap: u64,
    pub period_cap: u64,
    pub period_seconds: i64,
    pub expires_at: i64,
}

#[event]
pub struct PolicyUpdated {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub per_payment_cap: u64,
    pub period_cap: u64,
    pub period_seconds: i64,
    pub expires_at: i64,
}

#[event]
pub struct RecipientAdded {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub recipient: Pubkey,
    pub allowlist_len: u8,
}

#[event]
pub struct RecipientRemoved {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub recipient: Pubkey,
    pub allowlist_len: u8,
}

#[event]
pub struct Spent {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
    pub spent_in_period: u64,
}
