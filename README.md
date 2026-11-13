# mandate

On-chain spend limits for an agent's payments.

## The problem

An agent that pays for APIs, data or compute holds a key that can move the whole
balance. The usual fix is a limit inside the agent's own code, or inside the
server that sponsors the transaction. Both are the same trust assumption: the
party being limited also enforces the limit. A prompt injection, a bad deploy or
one unguarded code path and the key spends everything it can reach.

`mandate` moves the limit into a program. The owner signs once and names an
agent key plus a policy. Every later payment is checked by the chain, so the
agent cannot exceed what it was given even if the agent itself is compromised.

## Policy

A mandate binds one agent key to one mint with four limits:

- `per_payment_cap`: the largest single payment
- `period_cap`: the most that can be spent inside one period
- `period_seconds`: the length of that period, reset lazily on the first spend
  that would overflow it
- `expires_at`: a hard stop, after which nothing is payable
- `allowlist`: the recipients that may be paid, capped at 8

The owner keeps the remaining balance, because the vault is a token account
owned by the mandate PDA and `revoke` returns the remainder to the owner.

## Status

Implemented and tested against LiteSVM:

- `create_mandate`: validates the policy and writes the `Mandate` account
- `spend`: agent-signed payment, enforces the caps, the allowlist and the expiry,
  and moves tokens from the mandate vault
- `revoke`: owner-signed, returns the vault remainder and closes the accounts

It is deployed on devnet at `9izrkU3aCb3KxAcfzErm91GCSnyFEsqTkQHgKWb4hTYN`, under
upgrade authority `TKAmDM9kyPicm7T72mQxWwB4Npf3RdJdw4BhvqFkvBV` (the key itself
is not in this repo). Its IDL is published on chain, so
`anchor idl fetch 9izrkU3aCb3KxAcfzErm91GCSnyFEsqTkQHgKWb4hTYN --provider.cluster devnet`
returns the interface without the repo.

`clients/devnet` has been run against that deployment, and all three
instructions have executed there: a payment of 1,500,000 units moved from the
vault to the merchant, a 3,000,000 unit payment was refused on chain with
`CapExceeded`, and `revoke` returned the remaining 1,000,000 units to the owner
and closed both the vault and the mandate account.

There is no instruction to top up a vault; the owner funds and refunds it with
a plain SPL transfer to the token account the mandate controls.

## Layout

```
programs/mandate/
  src/lib.rs                     instruction entrypoints
  src/state.rs                   Mandate account, MandatePolicy argument, events
  src/error.rs                   typed rejections
  src/constants.rs               PDA seed, allowlist bound
  src/instructions/create_mandate.rs
  src/instructions/spend.rs
  src/instructions/revoke.rs
  tests/support/mod.rs           shared LiteSVM fixture
  tests/create_mandate.rs
  tests/spend.rs
  tests/revoke.rs
  tests/fixtures/spl_token.so    token program loaded into the test VM
```

The mandate PDA is derived from `["mandate", owner, agent]`, so one owner can
give one agent one mandate per mint and no one else can create or overwrite it.
The bump is not stored in the account; it is re-derived from the seeds every
time the account is used, so a stale bump can never be trusted.

## Build and test

Requires the Rust toolchain `1.89.0` (pinned in `rust-toolchain.toml`), the
Solana platform-tools, and Anchor `1.2.2`.

```
anchor build
cargo test
```

The tests run against the compiled `.so` in a LiteSVM instance, so they need
`anchor build` to have produced `target/deploy/mandate.so` first. `cargo test`
currently runs 27 tests: 7 unit tests over the period accounting and 20
integration tests that send real transactions.

LiteSVM ships no SPL token program, so `tests/fixtures/spl_token.so` is loaded
alongside the program. It was dumped from mainnet:

```
solana program dump -u m TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA spl_token.so
```

The tests run against the `.so` in `target/deploy`, so they exercise the same
artifact that gets deployed. If a deploy is rejected with `invalid file header`,
`anchor build` wrote an artifact that the cluster's loader rejects. On this
toolchain it builds into `target/sbpfv3-solana-solana`, whose output carries ELF
machine `0x00f7`; the loader wants `0x0107`. Building with plain
`cargo-build-sbf` produces the accepted one, so the deploy step always follows
`anchor build` with it and checks the header:

```
anchor build
(cd programs/mandate && cargo-build-sbf --sbf-out-dir ../../target/deploy)
xxd -s 18 -l 2 target/deploy/mandate.so   # expect 0701
```

## Deploying

```
solana program deploy target/deploy/mandate.so \
  --program-id target/deploy/mandate-keypair.json \
  --upgrade-authority <authority-keypair> \
  --url <cluster>
```

The program keypair in `target/deploy` must stay with the repo's `declare_id`.
The upgrade authority is the key that can replace this program, so it belongs in
cold storage, not next to the deploy script.

`clients/devnet` is a small binary that drives a cluster end to end: it creates
a mint, funds a vault, creates a mandate for a fresh agent key, sends one
payment inside the policy, one above the per-payment cap that the program must
refuse, then revokes and confirms the vault and mandate were closed. It keeps
its keys under `target/devnet/`, so a rerun reuses them and starts from a fresh
mandate.

```
cd clients/devnet
MANDATE_RPC_URL=https://api.devnet.solana.com cargo run
```

That crate has its own workspace and pins Rust 1.100, because `solana-rpc-client`
requires 1.98 while this program pins 1.89 for the SBF toolchain.

## Known limitations

- The period boundary comes from the `Clock` sysvar. Period accounting is
  correct relative to chain time, not wall-clock time, and a validator that
  controls its own clock can influence when a period rolls over.
- Caps are raw token units. The caller applies the mint's decimals; this program
  does not read mint metadata.
- There is no cap on the number of periods, so a long-lived mandate with a short
  period can record many resets. Each reset is a single field write.
- `spend` accepts any token account whose mint matches and whose authority is the
  mandate PDA, rather than one fixed vault address. An owner who creates several
  such accounts can split the balance across them, but `revoke` only sweeps the
  vault it is given and closes that one.
- The recipient is checked through the token account's owner, so a payment to a
  token account controlled by an allowlisted key is allowed even if the account
  itself was not named.
