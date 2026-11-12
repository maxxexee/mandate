use anchor_lang::prelude::*;

/// Seed prefix for the mandate PDA, derived per (owner, agent) pair.
#[constant]
pub const MANDATE_SEED: &[u8] = b"mandate";

/// Maximum recipients a single mandate can pay.
///
/// Must stay in sync with the `#[max_len]` on `Mandate::allowlist`, which needs
/// a literal. The instruction also checks the bound at runtime because the
/// argument arrives as an unbounded vector.
pub const MAX_ALLOWLIST: usize = 8;
