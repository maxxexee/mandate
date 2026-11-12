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

Implemented and tested:

- `create_mandate`: validates the policy and writes the `Mandate` account

Not implemented yet:

- `spend`: agent-signed payment, enforces the caps, the allowlist and the expiry,
  and moves tokens from the mandate vault
- `revoke`: owner-signed, returns the vault remainder and closes the accounts

The README will say otherwise once those exist. Nothing here is deployed yet.

## Layout

```
programs/mandate/
  src/lib.rs                     instruction entrypoints
  src/state.rs                   Mandate account, MandatePolicy argument, events
  src/error.rs                   typed rejections
  src/constants.rs               PDA seed, allowlist bound
  src/instructions/create_mandate.rs
  tests/create_mandate.rs        LiteSVM tests
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
`anchor build` to have produced `target/deploy/mandate.so` first.

## Known limitations

- The period boundary comes from the `Clock` sysvar. Period accounting is
  correct relative to chain time, not wall-clock time, and a validator that
  controls its own clock can influence when a period rolls over.
- Caps are raw token units. The caller applies the mint's decimals; this program
  does not read mint metadata.
- There is no cap on the number of periods, so a long-lived mandate with a short
  period can record many resets. Each reset is a single field write.
