//! Submitting transactions into the LiteSVM instance, with the failure logs a
//! test asserts on.

use {
    super::Fixture,
    anchor_lang::solana_program::instruction::Instruction,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

impl Fixture {
    pub(crate) fn expect_ok(&mut self, ixs: &[Instruction], signers: &[Keypair]) {
        self.submit(ixs, signers)
            .unwrap_or_else(|logs| panic!("expected the transaction to succeed, got:\n{logs}"));
    }

    pub(crate) fn expect_rejection(
        &mut self,
        ixs: &[Instruction],
        signers: &[Keypair],
        error_name: &str,
    ) {
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
    pub(crate) fn submit(
        &mut self,
        ixs: &[Instruction],
        signers: &[Keypair],
    ) -> Result<(), String> {
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
