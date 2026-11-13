use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("per_payment_cap must be greater than zero")]
    ZeroPerPaymentCap,
    #[msg("per_payment_cap cannot exceed period_cap")]
    PerPaymentCapExceedsPeriodCap,
    #[msg("period_seconds must be greater than zero")]
    ZeroPeriod,
    #[msg("expires_at must be in the future")]
    ExpiryInPast,
    #[msg("allowlist cannot be empty")]
    EmptyAllowlist,
    #[msg("allowlist exceeds the maximum of 8 recipients")]
    AllowlistTooLong,
    #[msg("agent cannot be the owner")]
    AgentIsOwner,
    #[msg("payment amount must be greater than zero")]
    ZeroAmount,
    #[msg("payment exceeds per_payment_cap")]
    CapExceeded,
    #[msg("payment would exceed the cap for this period")]
    PeriodExhausted,
    #[msg("recipient is not on the mandate allowlist")]
    NotAllowlisted,
    #[msg("mandate has expired")]
    Expired,
}
