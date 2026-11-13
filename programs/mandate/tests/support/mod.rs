//! LiteSVM fixture shared by the integration tests.
//!
//! Token accounts are written as raw state instead of being created by
//! `InitializeAccount`, so a test pays for the transaction under test only.
//! The bytes below are the SPL token account layout: mint, owner, amount, then
//! the fields this program never reads.

#![allow(dead_code)]

mod run;
mod token;

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::instruction::AccountMeta,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mandate::state::{Mandate, MandatePolicy, PolicyUpdate},
    solana_clock::Clock,
    solana_keypair::Keypair,
};

pub use solana_signer::Signer;

pub(crate) const PER_PAYMENT_CAP: u64 = 2_000_000;
pub(crate) const PERIOD_CAP: u64 = 5_000_000;
pub(crate) const PERIOD_SECONDS: i64 = 86_400;
/// Year 2096. Any value past the SVM clock works, this one will not age out.
pub(crate) const EXPIRES_AT: i64 = 4_000_000_000;

pub(crate) struct Fixture {
    pub svm: LiteSVM,
    pub program_id: Pubkey,
    pub owner: Keypair,
    pub agent: Keypair,
    pub mint: Pubkey,
    pub allowlist: Vec<Pubkey>,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let program_id = mandate::id();
        let mut svm = LiteSVM::new();
        svm.add_program(
            program_id,
            include_bytes!(concat!(
                env!("CARGO_TARGET_TMPDIR"),
                "/../deploy/mandate.so"
            )),
        )
        .unwrap();
        svm.add_program(
            anchor_spl::token::ID,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/spl_token.so"
            )),
        )
        .unwrap();

        let owner = Keypair::new();
        let agent = Keypair::new();
        svm.airdrop(&owner.pubkey(), 10_000_000_000).unwrap();
        svm.airdrop(&agent.pubkey(), 10_000_000_000).unwrap();

        Self {
            svm,
            program_id,
            owner,
            agent,
            mint: Pubkey::new_unique(),
            allowlist: vec![Pubkey::new_unique(), Pubkey::new_unique()],
        }
    }

    pub(crate) fn mandate_address(&self) -> Pubkey {
        mandate_address(&self.program_id, &self.owner.pubkey(), &self.agent.pubkey())
    }

    /// The canonical token account for the mandate, used as the vault.
    pub(crate) fn vault_address(&self) -> Pubkey {
        self.token_account_of(&self.mandate_address())
    }

    pub(crate) fn token_account_of(&self, authority: &Pubkey) -> Pubkey {
        anchor_spl::associated_token::get_associated_token_address(authority, &self.mint)
    }

    pub(crate) fn policy(&self) -> MandatePolicy {
        MandatePolicy {
            mint: self.mint,
            per_payment_cap: PER_PAYMENT_CAP,
            period_cap: PERIOD_CAP,
            period_seconds: PERIOD_SECONDS,
            expires_at: EXPIRES_AT,
            allowlist: self.allowlist.clone(),
        }
    }

    pub(crate) fn create_ix(&self, policy: MandatePolicy) -> Instruction {
        self.create_ix_with_agent(self.agent.pubkey(), policy)
    }

    /// Builds a create for an arbitrary agent key, so a test can propose a
    /// policy the program should refuse.
    pub(crate) fn create_ix_with_agent(&self, agent: Pubkey, policy: MandatePolicy) -> Instruction {
        let owner = self.owner.pubkey();
        Instruction::new_with_bytes(
            self.program_id,
            &mandate::instruction::CreateMandate { agent, policy }.data(),
            mandate::accounts::CreateMandate {
                owner,
                mandate: mandate_address(&self.program_id, &owner, &agent),
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    }

    pub(crate) fn create_mandate(&mut self) {
        let ix = self.create_ix(self.policy());
        self.expect_ok(&[ix], &[self.owner.insecure_clone()]);
    }

    pub(crate) fn spend_ix(&self, amount: u64, recipient_token: Pubkey) -> Instruction {
        self.spend_ix_as(self.agent.pubkey(), amount, recipient_token)
    }

    /// Builds a spend for an arbitrary agent key, so a test can try to spend
    /// with a signer the mandate was not created for.
    pub(crate) fn spend_ix_as(
        &self,
        agent: Pubkey,
        amount: u64,
        recipient_token: Pubkey,
    ) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &mandate::instruction::Spend { amount }.data(),
            mandate::accounts::Spend {
                agent,
                owner: self.owner.pubkey(),
                mandate: self.mandate_address(),
                vault: self.vault_address(),
                recipient_token,
                token_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
        )
    }

    pub(crate) fn update_policy_ix(&self, update: PolicyUpdate) -> Instruction {
        self.update_policy_ix_as(self.owner.pubkey(), update)
    }

    /// Builds an edit for an arbitrary owner key, so a test can try to change a
    /// mandate it does not own.
    pub(crate) fn update_policy_ix_as(&self, owner: Pubkey, update: PolicyUpdate) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &mandate::instruction::UpdatePolicy { update }.data(),
            self.edit_accounts_as(owner),
        )
    }

    pub(crate) fn add_recipient_ix(&self, recipient: Pubkey) -> Instruction {
        self.add_recipient_ix_as(self.owner.pubkey(), recipient)
    }

    pub(crate) fn add_recipient_ix_as(&self, owner: Pubkey, recipient: Pubkey) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &mandate::instruction::AddRecipient { recipient }.data(),
            self.edit_accounts_as(owner),
        )
    }

    pub(crate) fn remove_recipient_ix(&self, recipient: Pubkey) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &mandate::instruction::RemoveRecipient { recipient }.data(),
            self.edit_accounts_as(self.owner.pubkey()),
        )
    }

    fn edit_accounts_as(&self, owner: Pubkey) -> Vec<AccountMeta> {
        mandate::accounts::EditMandate {
            owner,
            agent: self.agent.pubkey(),
            mandate: self.mandate_address(),
        }
        .to_account_metas(None)
    }

    /// Funds the canonical vault of the mandate.
    pub(crate) fn fund_vault(&mut self, amount: u64) {
        let vault = self.vault_address();
        self.set_token_account(vault, &self.mandate_address(), amount);
    }

    /// Builds a spend that names an arbitrary vault, so a test can offer one
    /// that is not the mandate's associated token account.
    pub(crate) fn spend_ix_with_vault(
        &self,
        amount: u64,
        vault: Pubkey,
        recipient_token: Pubkey,
    ) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &mandate::instruction::Spend { amount }.data(),
            mandate::accounts::Spend {
                agent: self.agent.pubkey(),
                owner: self.owner.pubkey(),
                mandate: self.mandate_address(),
                vault,
                recipient_token,
                token_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
        )
    }

    pub(crate) fn revoke_ix(&self) -> Instruction {
        self.revoke_ix_as(self.owner.pubkey())
    }

    /// Builds a revoke against the real mandate for an arbitrary owner key, so
    /// a test can try to shut down someone else's mandate.
    pub(crate) fn revoke_ix_as(&self, owner: Pubkey) -> Instruction {
        Instruction::new_with_bytes(
            self.program_id,
            &mandate::instruction::Revoke {}.data(),
            mandate::accounts::Revoke {
                owner,
                agent: self.agent.pubkey(),
                mandate: self.mandate_address(),
                vault: self.vault_address(),
                owner_token: self.token_account_of(&owner),
                token_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
        )
    }

    pub(crate) fn set_clock(&mut self, unix_timestamp: i64) {
        let mut clock = self.svm.get_sysvar::<Clock>();
        clock.unix_timestamp = unix_timestamp;
        self.svm.set_sysvar::<Clock>(&clock);
    }

    pub(crate) fn read_mandate(&self) -> Mandate {
        let account = self
            .svm
            .get_account(&self.mandate_address())
            .expect("mandate account exists");
        let mut data: &[u8] = &account.data;
        Mandate::try_deserialize(&mut data).unwrap()
    }
}

pub(crate) fn mandate_address(program_id: &Pubkey, owner: &Pubkey, agent: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            mandate::constants::MANDATE_SEED,
            owner.as_ref(),
            agent.as_ref(),
        ],
        program_id,
    )
    .0
}
