//! Raw SPL token account state for the fixture.
//!
//! Accounts are written directly instead of going through `InitializeAccount`,
//! so a test pays for the transaction under test only. The bytes are the SPL
//! token account layout: mint, owner, amount, then fields this program ignores.

use {super::Fixture, anchor_lang::prelude::Pubkey, solana_account::Account};

const TOKEN_ACCOUNT_LEN: usize = 165;
const TOKEN_AMOUNT_OFFSET: usize = 64;
const TOKEN_STATE_OFFSET: usize = 108;
const TOKEN_STATE_INITIALIZED: u8 = 1;
const TOKEN_ACCOUNT_LAMPORTS: u64 = 2_039_280;

impl Fixture {
    /// Writes a token account holding `amount` and owned by `authority`.
    pub(crate) fn set_token_account(&mut self, address: Pubkey, authority: &Pubkey, amount: u64) {
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

    pub(crate) fn token_amount(&self, address: &Pubkey) -> u64 {
        let account = self.svm.get_account(address).expect("token account exists");
        let mut amount = [0u8; 8];
        amount.copy_from_slice(&account.data[TOKEN_AMOUNT_OFFSET..TOKEN_AMOUNT_OFFSET + 8]);
        u64::from_le_bytes(amount)
    }
}
