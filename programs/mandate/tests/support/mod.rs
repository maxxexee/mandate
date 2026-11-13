//! LiteSVM fixture shared by the integration tests.
//!
//! Token accounts are written as raw state instead of being created by
//! `InitializeAccount`, so a test pays for the transaction under test only.
//! The bytes below are the SPL token account layout: mint, owner, amount, then
//! the fields this program never reads.

#![allow(dead_code)]

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mandate::state::{Mandate, MandatePolicy},
    solana_account::Account,
    solana_clock::Clock,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_transaction::versioned::VersionedTransaction,
};

pub use solana_signer::Signer;

pub const PER_PAYMENT_CAP: u64 = 2_000_000;
pub const PERIOD_CAP: u64 = 5_000_000;
pub const PERIOD_SECONDS: i64 = 86_400;
/// Year 2096. Any value past the SVM clock works, this one will not age out.
pub const EXPIRES_AT: i64 = 4_000_000_000;

const TOKEN_ACCOUNT_LEN: usize = 165;
const TOKEN_AMOUNT_OFFSET: usize = 64;
const TOKEN_STATE_OFFSET: usize = 108;
const TOKEN_STATE_INITIALIZED: u8 = 1;
const TOKEN_ACCOUNT_LAMPORTS: u64 = 2_039_280;

pub struct Fixture {
    pub svm: LiteSVM,
    pub program_id: Pubkey,
    pub owner: Keypair,
    pub agent: Keypair,
    pub mint: Pubkey,
    pub allowlist: Vec<Pubkey>,
}

impl Fixture {
    pub fn new() -> Self {
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

    pub fn mandate_address(&self) -> Pubkey {
        mandate_address(&self.program_id, &self.owner.pubkey(), &self.agent.pubkey())
    }

    /// The canonical token account for the mandate, used as the vault.
    pub fn vault_address(&self) -> Pubkey {
        self.token_account_of(&self.mandate_address())
    }

    pub fn token_account_of(&self, authority: &Pubkey) -> Pubkey {
        anchor_spl::associated_token::get_associated_token_address(authority, &self.mint)
    }

    pub fn policy(&self) -> MandatePolicy {
        MandatePolicy {
            mint: self.mint,
            per_payment_cap: PER_PAYMENT_CAP,
            period_cap: PERIOD_CAP,
            period_seconds: PERIOD_SECONDS,
            expires_at: EXPIRES_AT,
            allowlist: self.allowlist.clone(),
        }
    }

    pub fn create_ix(&self, policy: MandatePolicy) -> Instruction {
        self.create_ix_with_agent(self.agent.pubkey(), policy)
    }

    /// Builds a create for an arbitrary agent key, so a test can propose a
    /// policy the program should refuse.
    pub fn create_ix_with_agent(&self, agent: Pubkey, policy: MandatePolicy) -> Instruction {
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

    pub fn create_mandate(&mut self) {
        let ix = self.create_ix(self.policy());
        self.expect_ok(&[ix], &[self.owner.insecure_clone()]);
    }

    pub fn spend_ix(&self, amount: u64, recipient_token: Pubkey) -> Instruction {
        self.spend_ix_as(self.agent.pubkey(), amount, recipient_token)
    }

    /// Builds a spend for an arbitrary agent key, so a test can try to spend
    /// with a signer the mandate was not created for.
    pub fn spend_ix_as(
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

    pub fn revoke_ix(&self) -> Instruction {
        self.revoke_ix_as(self.owner.pubkey())
    }

    /// Builds a revoke against the real mandate for an arbitrary owner key, so
    /// a test can try to shut down someone else's mandate.
    pub fn revoke_ix_as(&self, owner: Pubkey) -> Instruction {
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

    /// Writes a token account holding `amount` and owned by `authority`.
    pub fn set_token_account(&mut self, address: Pubkey, authority: &Pubkey, amount: u64) {
        let mut data = vec![0u8; TOKEN_ACCOUNT_LEN];
        data[0..32].copy_from_slice(self.mint.as_ref());
        data[32..64].copy_from_slice(authority.as_ref());
        data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8].copy_from_slice(&amount.to_le_bytes());
        data[TOKEN_STATE_OFFSET] = TOKEN_STATE_INITIALIZED;

        self.svm
            .set_account(
                address,
                Account {
                    lamports: TOKEN_ACCOUNT_LAMPORTS,
                    data,
                    owner: anchor_spl::token::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .unwrap();
    }

    pub fn token_amount(&self, address: &Pubkey) -> u64 {
        let account = self.svm.get_account(address).expect("token account exists");
        let mut amount = [0u8; 8];
        amount.copy_from_slice(&account.data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8]);
        u64::from_le_bytes(amount)
    }

    pub fn set_clock(&mut self, unix_timestamp: i64) {
        let mut clock = self.svm.get_sysvar::<Clock>();
        clock.unix_timestamp = unix_timestamp;
        self.svm.set_sysvar::<Clock>(&clock);
    }

    pub fn read_mandate(&self) -> Mandate {
        let account = self
            .svm
            .get_account(&self.mandate_address())
            .expect("mandate account exists");
        let mut data: &[u8] = &account.data;
        Mandate::try_deserialize(&mut data).unwrap()
    }

    pub fn expect_ok(&mut self, ixs: &[Instruction], signers: &[Keypair]) {
        self.submit(ixs, signers)
            .unwrap_or_else(|logs| panic!("expected the transaction to succeed, got:\n{logs}"));
    }

    pub fn expect_rejection(&mut self, ixs: &[Instruction], signers: &[Keypair], error_name: &str) {
        let logs = self
            .submit(ixs, signers)
            .expect_err("expected the transaction to be rejected");
        assert!(
            logs.contains(error_name),
            "expected {error_name} in logs, got:\n{logs}"
        );
    }

    /// Returns the joined program logs on failure, so a test can assert which
    /// check rejected the transaction rather than only that one did.
    pub fn submit(&mut self, ixs: &[Instruction], signers: &[Keypair]) -> Result<(), String> {
        let payer = signers
            .first()
            .expect("a transaction needs at least one signer")
            .pubkey();
        // Each submit stands in for a new block, so replaying the same
        // instruction twice is not silently deduped as AlreadyProcessed.
        self.svm.expire_blockhash();
        let blockhash = self.svm.latest_blockhash();
        let msg = Message::new_with_blockhash(ixs, Some(&payer), &blockhash);
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers)
            .map_err(|error| format!("could not build the transaction: {error:?}"))?;

        self.svm
            .send_transaction(tx)
            .map(|_| ())
            .map_err(|failure| {
                let logs = failure.meta.logs.join("\n");
                if logs.is_empty() {
                    format!("{:?}", failure.err)
                } else {
                    logs
                }
            })
    }
}

pub fn mandate_address(program_id: &Pubkey, owner: &Pubkey, agent: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[mandate::constants::MANDATE_SEED, owner.as_ref(), agent.as_ref()],
        program_id,
    )
    .0
}
