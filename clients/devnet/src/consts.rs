//! Values the demo run uses. Token amounts are in the mint's smallest unit.

pub(crate) const RPC_URL: &str = "https://api.devnet.solana.com";
pub(crate) const DECIMALS: u8 = 6;
pub(crate) const MINT_AMOUNT: u64 = 10_000_000;
pub(crate) const VAULT_FUNDING: u64 = 4_000_000;
pub(crate) const PAYMENT: u64 = 1_500_000;
pub(crate) const SECOND_PAYMENT: u64 = 500_000;
pub(crate) const OVER_CAP_PAYMENT: u64 = 3_000_000;
pub(crate) const PER_PAYMENT_CAP: u64 = 2_000_000;
pub(crate) const PERIOD_CAP: u64 = 5_000_000;
pub(crate) const PERIOD_SECONDS: i64 = 86_400;
pub(crate) const MANDATE_LIFETIME_SECONDS: i64 = 7 * 86_400;
pub(crate) const AGENT_FEE_LAMPORTS: u64 = 20_000_000;
