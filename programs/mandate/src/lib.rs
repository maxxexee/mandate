//! On-chain spend limits for an agent's payments.
//!
//! An owner binds a policy to an agent key in a PDA. A later payment is judged
//! against that policy by this program, not by the server that asked for it.

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("9izrkU3aCb3KxAcfzErm91GCSnyFEsqTkQHgKWb4hTYN");

#[program]
pub mod mandate {
    use super::*;

    pub fn create_mandate(
        ctx: Context<CreateMandate>,
        agent: Pubkey,
        policy: MandatePolicy,
    ) -> Result<()> {
        instructions::create_mandate::handle_create_mandate(ctx, agent, policy)
    }
}
