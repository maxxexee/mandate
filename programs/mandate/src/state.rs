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
pub struct Spent {
    pub owner: Pubkey,
    pub agent: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
    pub spent_in_period: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn mandate() -> Mandate {
        Mandate {
            owner: Pubkey::new_unique(),
            agent: Pubkey::new_unique(),
            mint: Pubkey::new_unique(),
            per_payment_cap: 2_000_000,
            period_cap: 5_000_000,
            period_seconds: DAY,
            period_start: 1_000,
            spent_in_period: 0,
            expires_at: 1_000_000,
            allowlist: vec![Pubkey::new_unique()],
        }
    }

    fn rejection(m: &mut Mandate, amount: u64, now: i64) -> String {
        m.charge(amount, now)
            .expect_err("expected the charge to be rejected")
            .to_string()
    }

    #[test]
    fn accumulates_within_the_period() {
        let mut m = mandate();
        m.charge(2_000_000, 1_100).unwrap();
        m.charge(1_000_000, 1_200).unwrap();
        assert_eq!(m.spent_in_period, 3_000_000);
        assert_eq!(m.period_start, 1_000);
    }

    #[test]
    fn rolls_the_period_over_once_it_elapses() {
        let mut m = mandate();
        m.charge(2_000_000, 1_100).unwrap();
        m.charge(2_000_000, 1_200).unwrap();
        m.charge(1_000_000, 1_300).unwrap();
        assert_eq!(m.spent_in_period, 5_000_000);

        m.charge(1_000_000, 1_000 + DAY).unwrap();
        assert_eq!(m.spent_in_period, 1_000_000);
        assert_eq!(m.period_start, 1_000 + DAY);
    }

    #[test]
    fn rejects_a_payment_above_the_per_payment_cap() {
        let mut m = mandate();
        assert!(rejection(&mut m, 2_000_001, 1_100).contains("CapExceeded"));
    }

    #[test]
    fn rejects_a_payment_that_would_exhaust_the_period() {
        let mut m = mandate();
        m.charge(2_000_000, 1_100).unwrap();
        m.charge(2_000_000, 1_200).unwrap();
        assert!(rejection(&mut m, 2_000_000, 1_300).contains("PeriodExhausted"));
        assert_eq!(m.spent_in_period, 4_000_000);
    }

    #[test]
    fn rejects_a_zero_payment() {
        let mut m = mandate();
        assert!(rejection(&mut m, 0, 1_100).contains("ZeroAmount"));
    }

    #[test]
    fn rejects_after_expiry_even_with_headroom() {
        let mut m = mandate();
        assert!(rejection(&mut m, 1_000_000, 1_000_000).contains("Expired"));
    }

    #[test]
    fn allows_only_listed_recipients() {
        let m = mandate();
        assert!(m.allows(&m.allowlist[0]));
        assert!(!m.allows(&Pubkey::new_unique()));
    }
}
