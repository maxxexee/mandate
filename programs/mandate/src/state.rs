use anchor_lang::prelude::*;

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
        require!(self.per_payment_cap > 0, crate::error::ErrorCode::ZeroPerPaymentCap);
        require!(
            self.per_payment_cap <= self.period_cap,
            crate::error::ErrorCode::PerPaymentCapExceedsPeriodCap
        );
        require!(self.period_seconds > 0, crate::error::ErrorCode::ZeroPeriod);
        require!(self.expires_at > now, crate::error::ErrorCode::ExpiryInPast);
        require!(
            !self.allowlist.is_empty(),
            crate::error::ErrorCode::EmptyAllowlist
        );
        require!(
            self.allowlist.len() <= crate::constants::MAX_ALLOWLIST,
            crate::error::ErrorCode::AllowlistTooLong
        );
        Ok(())
    }
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
