# zk-clob

A zero-knowledge central limit order book: a batch order-matching and
settlement engine that runs verifiably inside an
[SP1](https://github.com/succinctlabs/sp1) zkVM, with a Solidity contract
that only accepts a new exchange state when it is backed by a valid proof
of correct execution.

> **Status: research / MVP.** Single fixed fee, no order cancellation, no
> persistent order book across batches, escape mode is permanent once
> activated. See Known Vulnerabilities and TODO below for the full list.

## What it does

- Matches signed limit orders against a per-market book using price-time
  priority, charging a single fixed buyer-side fee.
- Commits account state as a path-compressed Patricia Merkle tree, so the
  zkVM only needs a proof over the accounts a batch actually touches, not
  the whole account set.
- Lets users deposit and withdraw ETH/ERC-20 through the operator, or
  force a withdrawal directly on-chain if the operator won't process one
  — and, if forced withdrawals are stalled long enough, escape the
  contract entirely with a self-service Merkle proof against the last
  known state.
- Every one of those state transitions is proven by the same Rust code
  the host runs, executed inside the SP1 zkVM; the contract advances its
  state root only after verifying that proof plus a set of on-chain
  cross-checks (config hash, batch ordering, deposit/withdrawal cursors).

## How it works

1. **Operator** (`zk-clob-host`) collects signed orders and withdrawal
   requests off-chain, reads pending deposits/forced withdrawals from the
   contract, and builds a `BatchInput` against its view of account state.
2. **SP1 guest** (`zk-clob-guest`) re-executes the exact same settlement
   logic (`zk-clob-core::settle_batch`) inside the zkVM — validating
   signatures, nonces, and balances; matching orders; applying deposits,
   withdrawals, and forced withdrawals — and commits a compact
   `PublicOutput`: old/new state roots, a config hash, and hashes binding
   the batch, trades, deposits, withdrawals, and forced withdrawals.
3. **Host** drives execution/proving of the guest and produces a Groth16
   proof plus those public values.
4. **`ZkClob` contract** verifies the proof, checks the batch starts from
   its current state root and the next expected batch ID, recomputes
   hashes of its own deposit/forced-withdrawal queues to confirm the
   batch consumed exactly what it claims, and — only if everything
   matches — advances `stateRoot` and pays out withdrawals.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the pieces fit together
internally (data model, the Patricia tree, hashing/signing scheme,
matching algorithm, conservation invariant) and
[PROTOCOL.md](PROTOCOL.md) for the step-by-step on-chain procedures
(deposits, withdrawals, forced withdrawals, the escape hatch).

## Repository layout

```
crates/
  core/         deterministic settlement engine (types, validation,
                matching, Merkle state, hashing) — shared by host and guest
  guest/        SP1 program: re-runs core's settle_batch() inside the zkVM
  host/         account tree + batch builder + CLI to execute/prove batches
  test-utils/   fixtures used by tests and the host CLI

contracts/      Foundry project: ZkClob settlement contract, single-leaf
                Patricia proof verifier for escape withdrawals, tests
testdata/       sample proof artifacts (regenerated via `make prove`)
```

The core crate is written to be usable unmodified inside the zkVM: no I/O,
no system time, no randomness, no floating point.

## Building and testing

Rust workspace:

```sh
cargo build --workspace
make test              # cargo test --workspace
make test.guest        # run the guest inside the SP1 executor (no proving)
make test.guest.real   # actually generate and verify a proof (slow, --ignored)
make execute           # run a fixture through the guest executor, print public output
make prove             # generate a real Groth16 proof for the happy-path fixture
```

Contracts (Foundry, via git submodules):

```sh
make setup.contract    # git submodule update --init --recursive
make test.contract     # forge test --root contracts
```

## Known vulnerabilities

Trust assumptions and griefing vectors a reader should know about before
relying on this beyond research/MVP use:

- **No data-availability guarantee.** Full account state lives only with
  the operator off-chain; the contract only ever sees Patricia proofs for
  the accounts a given batch touches. If the operator disappears without
  publishing its state, other parties may not have enough of the tree to
  reconstruct the side-node proof `escapeWithdraw` requires — even a user
  who remembers their own balance can be stuck without the rest of the
  tree.
- **Escape mode is a permanent, exchange-wide, unilaterally-triggerable
  halt.** Any single account can force it: request a forced withdrawal
  and let its deadline lapse (whether from operator malice, a bug, or
  ordinary downtime), then call `activateEscapeMode()`. Once active,
  `settle()` is blocked forever for every other user, with no path back
  to normal operation (see TODO below).
- **A single unprocessable forced withdrawal permanently blocks the FIFO
  queue behind it** — e.g. one to a contract address that reverts on
  receiving ETH. Since forced withdrawals must be consumed strictly in
  order, this is exactly the mechanism that can trigger the escape-mode
  griefing case above, deliberately or not.
- **Single centralized operator.** It alone decides which signed orders
  and withdrawals to include in a batch and can censor by omission; the
  forced-withdrawal/escape path is the only recourse, with the tradeoffs
  above.
- **`VERIFIER` and `PROGRAM_VKEY` are immutable** once the contract is
  deployed. A bug found in the guest program after deployment cannot be
  patched in place — fixing it means deploying a new contract and
  migrating funds.

## TODO

- data availability
- use indices instead of ids in guest
- persistent order book (remaining orders)
- parallel multi-market model (deltas -> root update)
- KZG-based vector-commitment tree
- user side withdrawals
- recovery path out of escape mode (currently permanent once activated)

## License

MIT
