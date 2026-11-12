use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, Space, ToAccountMetas,
    },
    litesvm::LiteSVM,
    mandate::state::{Mandate, MandatePolicy},
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const PER_PAYMENT_CAP: u64 = 2_000_000;
const PERIOD_CAP: u64 = 5_000_000;
const PERIOD_SECONDS: i64 = 86_400;
/// Year 2096. Any value past the SVM clock works, this one will not age out.
const EXPIRES_AT: i64 = 4_000_000_000;

struct Fixture {
    svm: LiteSVM,
    program_id: Pubkey,
    owner: Keypair,
    agent: Pubkey,
    mint: Pubkey,
    allowlist: Vec<Pubkey>,
}

fn fixture() -> Fixture {
    let program_id = mandate::id();
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/mandate.so"
    ));
    svm.add_program(program_id, bytes).unwrap();

    let owner = Keypair::new();
    svm.airdrop(&owner.pubkey(), 10_000_000_000).unwrap();

    Fixture {
        svm,
        program_id,
        agent: Pubkey::new_unique(),
        owner,
        mint: Pubkey::new_unique(),
        allowlist: vec![Pubkey::new_unique(), Pubkey::new_unique()],
    }
}

fn mandate_address(program_id: &Pubkey, owner: &Pubkey, agent: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[mandate::constants::MANDATE_SEED, owner.as_ref(), agent.as_ref()],
        program_id,
    )
    .0
}

fn policy(f: &Fixture) -> MandatePolicy {
    MandatePolicy {
        mint: f.mint,
        per_payment_cap: PER_PAYMENT_CAP,
        period_cap: PERIOD_CAP,
        period_seconds: PERIOD_SECONDS,
        expires_at: EXPIRES_AT,
        allowlist: f.allowlist.clone(),
    }
}

fn create_ix(f: &Fixture, policy: MandatePolicy) -> Instruction {
    let owner = f.owner.pubkey();
    Instruction::new_with_bytes(
        f.program_id,
        &mandate::instruction::CreateMandate {
            agent: f.agent,
            policy,
        }
        .data(),
        mandate::accounts::CreateMandate {
            owner,
            mandate: mandate_address(&f.program_id, &owner, &f.agent),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

/// Returns the joined program logs on failure, so a test can assert which
/// check rejected the transaction rather than only that one did.
fn submit(f: &mut Fixture, ix: Instruction) -> Result<(), String> {
    let blockhash = f.svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&f.owner.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&f.owner]).unwrap();

    f.svm.send_transaction(tx).map(|_| ()).map_err(|failure| failure.meta.logs.join("\n"))
}

fn expect_rejection(result: Result<(), String>, error_name: &str) {
    let logs = result.expect_err("expected the transaction to be rejected");
    assert!(
        logs.contains(error_name),
        "expected {error_name} in logs, got:\n{logs}"
    );
}

fn read_mandate(f: &Fixture) -> Mandate {
    let address = mandate_address(&f.program_id, &f.owner.pubkey(), &f.agent);
    let account = f.svm.get_account(&address).expect("mandate account exists");
    let mut data: &[u8] = &account.data;
    Mandate::try_deserialize(&mut data).unwrap()
}

#[test]
fn creates_a_mandate_that_matches_the_policy() {
    let mut f = fixture();
    let policy = policy(&f);
    let ix = create_ix(&f, policy.clone());

    assert!(submit(&mut f, ix).is_ok());

    let mandate = read_mandate(&f);
    assert_eq!(mandate.owner, f.owner.pubkey());
    assert_eq!(mandate.agent, f.agent);
    assert_eq!(mandate.mint, f.mint);
    assert_eq!(mandate.per_payment_cap, PER_PAYMENT_CAP);
    assert_eq!(mandate.period_cap, PERIOD_CAP);
    assert_eq!(mandate.period_seconds, PERIOD_SECONDS);
    assert_eq!(mandate.expires_at, EXPIRES_AT);
    assert_eq!(mandate.allowlist, policy.allowlist);
    assert_eq!(mandate.spent_in_period, 0);
}

#[test]
fn account_is_sized_for_the_full_allowlist() {
    let mut f = fixture();
    let mut policy = policy(&f);
    policy.allowlist = (0..mandate::constants::MAX_ALLOWLIST)
        .map(|_| Pubkey::new_unique())
        .collect();
    let ix = create_ix(&f, policy.clone());

    assert!(submit(&mut f, ix).is_ok());

    let address = mandate_address(&f.program_id, &f.owner.pubkey(), &f.agent);
    let account = f.svm.get_account(&address).unwrap();
    assert_eq!(account.data.len(), 8 + Mandate::INIT_SPACE);
    assert_eq!(read_mandate(&f).allowlist.len(), policy.allowlist.len());
}

#[test]
fn rejects_a_per_payment_cap_above_the_period_cap() {
    let mut f = fixture();
    let mut policy = policy(&f);
    policy.per_payment_cap = PERIOD_CAP + 1;
    let ix = create_ix(&f, policy);

    expect_rejection(submit(&mut f, ix), "PerPaymentCapExceedsPeriodCap");
}

#[test]
fn rejects_a_zero_per_payment_cap() {
    let mut f = fixture();
    let mut policy = policy(&f);
    policy.per_payment_cap = 0;
    let ix = create_ix(&f, policy);

    expect_rejection(submit(&mut f, ix), "ZeroPerPaymentCap");
}

#[test]
fn rejects_a_zero_period() {
    let mut f = fixture();
    let mut policy = policy(&f);
    policy.period_seconds = 0;
    let ix = create_ix(&f, policy);

    expect_rejection(submit(&mut f, ix), "ZeroPeriod");
}

#[test]
fn rejects_an_expiry_in_the_past() {
    let mut f = fixture();
    let mut policy = policy(&f);
    policy.expires_at = -1;
    let ix = create_ix(&f, policy);

    expect_rejection(submit(&mut f, ix), "ExpiryInPast");
}

#[test]
fn rejects_an_empty_allowlist() {
    let mut f = fixture();
    let mut policy = policy(&f);
    policy.allowlist = Vec::new();
    let ix = create_ix(&f, policy);

    expect_rejection(submit(&mut f, ix), "EmptyAllowlist");
}

#[test]
fn rejects_an_allowlist_over_the_bound() {
    let mut f = fixture();
    let mut policy = policy(&f);
    policy.allowlist = (0..=mandate::constants::MAX_ALLOWLIST)
        .map(|_| Pubkey::new_unique())
        .collect();
    let ix = create_ix(&f, policy);

    expect_rejection(submit(&mut f, ix), "AllowlistTooLong");
}

#[test]
fn rejects_the_owner_as_its_own_agent() {
    let mut f = fixture();
    f.agent = f.owner.pubkey();
    let ix = create_ix(&f, policy(&f));

    expect_rejection(submit(&mut f, ix), "AgentIsOwner");
}

#[test]
fn a_second_mandate_for_the_same_pair_cannot_be_created() {
    let mut f = fixture();
    let first = create_ix(&f, policy(&f));
    assert!(submit(&mut f, first).is_ok());

    let second = create_ix(&f, policy(&f));
    assert!(
        submit(&mut f, second).is_err(),
        "the mandate PDA must already be in use"
    );
}
