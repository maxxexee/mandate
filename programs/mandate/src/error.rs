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
}
