# Architecture

This document describes how zk-clob is put together internally: the data
model, the state commitment scheme, the batch settlement pipeline, and how
the pieces (core / guest / host / contracts) fit together. For the
end-to-end deposit/withdrawal/escape flows see [PROTOCOL.md](PROTOCOL.md);
for build/test commands see [README.md](README.md).

## Components

```
                     ┌──────────────────────────────────────────┐
                     │              Operator (host)             │
                     │  AccountTree → BatchBuilder → BatchInput │
                     └───────────────────┬──────────────────────┘
                                         │ SP1Stdin
                                         ▼
                     ┌──────────────────────────────────────────┐
                     │        zk-clob-guest (SP1 zkVM)          │
                     │   settle_batch(BatchInput) → BatchOutput │
                     │   commits abi_encode(PublicOutput)       │
                     └───────────────────┬──────────────────────┘
                                         │ Groth16 proof + public values
                                         ▼
                     ┌──────────────────────────────────────────┐
                     │       ZkClob.sol (Ethereum contract)     │
                     │  verify proof → check cursors/hashes →   │
                     │  advance stateRoot → pay out withdrawals │
                     └──────────────────────────────────────────┘
```

- **`zk-clob-core`** — the settlement engine (`settle_batch`) and every type
  it operates on. Pure, deterministic, no I/O. Compiled into both the host
  binary and the SP1 guest, so the same code that produces a batch's
  effects is the code being proven.
- **`zk-clob-guest`** — a `#![no_main]` SP1 program. Reads a `BatchInput`
  from zkVM stdin, calls `settle_batch`, commits the ABI-encoded
  `PublicOutput` as the proof's public values.
- **`zk-clob-host`** — off-chain operator tooling: `AccountTree` (the full
  account state), `BatchBuilder` (accumulates and pre-validates one batch's
  operations), and a CLI (`execute` / `prove`) that drives the SP1
  `ProverClient`.
- **`zk-clob-test-utils`** — shared fixtures (signing keypairs, asset/market
  configs, two canned `BatchInput`s) used by core, host, and guest tests.
- **`contracts/`** — `ZkClob.sol` (settlement + deposit/forced-withdrawal
  queues + escape hatch), `PatriciaProof.sol` (single-leaf Merkle proof
  verifier used only in escape mode), `IZkClob.sol` (interface, error and
  event definitions).

## Batch lifecycle

1. The operator holds the full account set in an `AccountTree`. For a new
   batch it opens a `BatchBuilder` against that tree plus the current
   `ExchangeConfig`, and feeds it deposits, forced withdrawals, orders, and
   withdrawals one at a time. Each `BatchBuilder` method re-implements the
   same checks `settle_batch` will later enforce (cursor sequencing, zero
   amounts, known assets/markets, nonce ordering, per-batch limits) so bad
   input is rejected off-chain, cheaply, before a proof is ever attempted.
2. `BatchBuilder::build` produces a `BatchInput`: a `StateWitness` (only the
   touched accounts, plus a Patricia multiproof against the full tree),
   the operations for this batch, per-market `MarketOrderBook`s (order
   indices split into buy/sell sides, sorted by price-time priority), and
   the `ExchangeConfig`.
3. The host CLI (`execute`/`prove`) serializes `BatchInput` into `SP1Stdin`
   and runs it through the SP1 guest, which calls `settle_batch` and
   commits the resulting `PublicOutput`.
4. `settle_batch` (`crates/core/src/settlement/mod.rs`), in order:
   - `validate_limits` — per-batch element counts against `MAX_*` constants
     in `consts.rs` (guards against unbounded input inflating the zkVM
     trace).
   - `validate_config`, `validate_accounts` — config and account-list
     well-formedness (sorted, no duplicates, fee recipient exists, etc).
   - `validate_deposits` / `validate_forced_withdrawals` — cursors must be
     exactly sequential starting from the old cursor.
   - Recomputes `state.root()` from the witness and checks it equals
     `expected_old_state_root` — this is what ties the batch to a specific
     on-chain state.
   - Computes `config_hash`, `domain_hash`, `batch_hash` (a domain-separated
     hash over every order, binding the whole order set into one
     committed value) and `consumed_forced_withdrawals_hash`.
   - Snapshots a conservation baseline (`AssetTracker`): sum of all touched
     account balances, plus incoming deposits, minus outgoing withdrawals
     and forced withdrawals.
   - Applies deposits and forced withdrawals to the account set.
   - `validate_withdrawals` / `validate_orders` — signature recovery
     against the domain hash, known account/market/asset, non-zero
     amounts.
   - `validate_nonces` — every order and withdrawal from the same account
     consumes the account's `next_nonce` sequentially, no gaps or repeats.
   - Applies withdrawals, advances nonces (`consume_nonces`).
   - `build_validated_books` re-validates the host-supplied order indices
     (right market, right side, strictly by price-time priority, every
     order index used exactly once) and hands `match_and_settle` immutable
     slices to match.
   - `match_and_settle` runs price-time-priority matching per market and
     mutates account balances directly for each fill (debit/credit).
   - Re-sums the conservation tracker over the now-updated accounts; a
     mismatch here means a bug in the settlement logic caused value to
     appear or vanish, and aborts the whole batch.
   - Recomputes the new state root and packages everything into
     `BatchOutput` (`build_output`).
5. The proof and its public values are handed to `ZkClob.settle`. The
   contract independently recomputes hashes of its own deposit and forced
   withdrawal queues over the claimed cursor range and compares them
   against the committed hashes — the guest never sees the contract's
   queues directly, so this is what proves the batch actually consumed the
   deposits/forced withdrawals the chain says it did, not some other set.
   Only after every check passes does it call the SP1 verifier, then
   commit the new root and pay out `withdrawals`/`forcedWithdrawals`.

## Data model (`crates/core/src/types`)

- **`AccountId` / `AssetId` / `MarketId`** — newtypes over `Address`/`B256`.
  `AccountId` doubles as both the Patricia key and the sparse-Merkle key
  (160-bit, MSB-first bit order matching Ethereum addresses).
- **`Account`** — an `AccountId`, a canonically-sorted `Vec<AssetBalance>`
  (no duplicates, no zero balances — enforced by `debit`/`credit`, which
  remove a balance entry when it hits zero and reject unknown assets), and
  `next_nonce`.
- **`Order` / `SignedOrder` / `SequencedOrder`** — an `Order` (market,
  side, price, quantity, nonce) is wrapped in `SignedOperation<Order>`
  (adds signer + secp256k1 signature) once signed by the trader, then in
  `SequencedOrder` once the operator assigns it a per-batch `sequence`
  (used only to break same-price ties — it is not part of the signed
  payload, so operators can't invalidate a signature by resequencing).
- **`Withdrawal` / `SignedWithdrawal` / `ExecutedWithdrawal`** — same
  signed-operation pattern; `ExecutedWithdrawal` is the canonical record
  that gets hashed into `withdrawalsHash` and paid out on-chain.
- **`Deposit`** / **`ForcedWithdrawal`** — on-chain-sourced, unsigned,
  identified by a strictly sequential `id` (the queue cursor).
- **`ExchangeConfig`** — `AssetConfig`s (asset ID + scale), `MarketConfig`s
  (base/quote asset pair), and one global `FeeConfig` (recipient + buyer
  fee in bps). Hashed as `config_hash`, which the contract pins immutably
  at deployment — so market/fee changes require a new contract, not a
  guest-side config update.
- **`Trade`** — the result of matching one buy against one sell: price,
  quantity, quote amount, and fee. Fees are charged to the buyer only, in
  the quote asset, at a fixed `buyer_fee_bps`.

## State commitment: Patricia Merkle tree

Account state is committed with a path-compressed binary Patricia tree
(`crates/core/src/trees/patricia.rs`), keyed on the 160-bit `AccountId`:

- Each node commits to a `(root, min_key, max_key)` triple. Internal nodes
  don't store both children recursively in the proof — a *canonical
  subtree* not touched by the current operation is represented by a single
  `PatriciaSubtree { root, min_key, max_key }`, so a multiproof over `k`
  touched accounts in an `n`-account tree is `O(k log(n/k))` rather than
  `O(n)`.
- This is what makes `StateWitness` (touched accounts + multiproof) usable
  as the guest's only view of state: the guest calls
  `PatriciaMerkleTree::compute_root_from_proof`, which walks the supplied
  leaves and side nodes and must reconstruct exactly
  `expected_old_state_root`. Because side-node ranges are checked to be
  disjoint from and non-overlapping with every leaf key
  (`selected_key_cannot_be_hidden_in_side_node` in the test suite), a
  malicious host can't hide an account inside a side node to freeze its
  balance while still passing verification.
- The same multiproof (re-supplied with updated leaf values) also verifies
  the *new* root after `settle_batch` mutates the touched accounts — one
  proof authenticates both the pre-state and, once leaves are updated,
  the post-state, which is why `StateWitness::root()` is called twice.
- `PatriciaProof.sol` is a from-scratch Solidity port of the same scheme,
  but specialized to a *single* leaf: since there's only one target key,
  side nodes can be folded in one pass ordered by split depth (deepest
  first) instead of the general recursive divide-and-conquer the Rust
  multi-key version needs. This equivalence is property-tested in
  `patricia.rs` (`single_leaf_fold_*` tests) against the general
  algorithm before being trusted in Solidity, since Solidity has no
  independent implementation to cross-check against. It's the only tree
  logic that exists on-chain, used solely by `escapeWithdraw`.
- `trees/dmt.rs` (dense Merkle, fixed power-of-two width) and `trees/smt.rs`
  (fully sparse, one leaf per key) are alternative/earlier tree
  implementations still built and tested but not wired into `State` —
  Patricia is the one actually used for account commitments.

## Hashing and domain separation

Every hashed structure implements `Sha256Hash::update_hash` (feeds its
fields into a running `Sha256`) and, for top-level objects,
`DomainSha256Hash::hash`, which first writes a fixed ASCII domain tag
(e.g. `b"ZKCLOB_ACCOUNT_V1"`) before the content. This prevents
cross-type hash collisions (an `Account` and a `Trade` that happen to
serialize to the same bytes still hash differently) and versions the
encoding — bumping a domain tag is how the format is meant to evolve
without silently colliding with the old one. `hashing.rs` and each type
module define one domain constant per hashed type; Solidity mirrors the
matching domain tags as `bytes` constants (see `DEPOSITS_HASH_DOMAIN` etc.
in `ZkClob.sol` and the `*_DOMAIN` constants in `PatriciaProof.sol`) so
both sides hash identically.

## Signing

Orders and withdrawals are signed as `SignedOperation<O>`. The signed
digest is `sha256("ZKCLOB_SIGNED_OPERATION_V1" || domain_hash ||
operation.hash())`, recovered with secp256k1 ECDSA
(`Signature::recover`, requiring a low-`s`, normalized signature) to an
Ethereum-style address (keccak256 of the uncompressed public key, low 20
bytes). `domain_hash` is `SigningDomain{protocol_version, chain_id,
exchange_id}.hash()` — binding a signature to one deployed contract on one
chain, so a signed order can't be replayed against a different
deployment. Note this is a custom SHA-256 scheme, not EIP-712 signing,
despite `PublicOutput`/`SigningDomain` being defined via `alloy_sol_types`
`sol!` for their Solidity ABI encoding — that macro only governs how
`PublicOutput` is ABI-encoded/decoded across the guest/contract boundary,
not how orders are signed.

## Order matching

The host groups a batch's orders per market into a `MarketOrderBook`
(indices into the flat `orders` array, split by side) sorted by
`Side::compare_priority` — best price first, then lowest `sequence` (the
operator-assigned arrival order) as tiebreaker. The guest doesn't trust
this ordering: `build_validated_books` re-derives it, rejecting any book
whose orders aren't strictly sorted by priority, aren't all on the
declared side/market, or don't cover every order index exactly once.
`match_market` then walks both sides with a two-pointer sweep, filling at
the resting order's price (whichever side arrived first by sequence),
splitting the remainder back into the book for the next match, until
either side runs out or the best bid drops below the best ask. Self-trades
(same trader on both sides) are rejected outright rather than silently
netted.

## Conservation invariant

Beyond per-operation balance checks (`debit` rejects insufficient funds),
`settle_batch` independently tracks total value per asset across the
whole batch (`AssetTracker` in `settlement/assets.rs`): starting balances
+ deposits − withdrawals − forced withdrawals must equal ending balances,
exactly, for every asset that appears. This is a global sanity check
orthogonal to the individual debit/credit bookkeeping — it would catch a
bug in trade settlement or fee accounting that balanced individual debits
and credits incorrectly but still let the sum drift.

## Escape hatch

If the operator stops processing forced withdrawals past their deadline
(`FORCED_WITHDRAWAL_DELAY`), anyone can call `activateEscapeMode()`, which
permanently freezes `stateRoot` and blocks `settle()`. From then on, each
account calls `escapeWithdraw` once, presenting its own leaf (balances +
nonce) and a `PatriciaProof` multiproof against the frozen root; the
contract reconstructs the root itself (`PatriciaProof.verifyAccount`) and,
if it matches, pays out every balance in the account directly. Because the
proof and destination account are fully public and funds only ever move to
the account named in the leaf, the call is permissionless — anyone can
submit it on a stuck account's behalf.

## Testing

- **Property tests** (`proptest`, in `trees/patricia.rs`, `trees/smt.rs`)
  fuzz multiproof construction/reconstruction across random key sets and
  updates, and cross-check the Solidity-equivalent single-leaf fold
  against the general algorithm.
- **Unit tests** are colocated with each module (hashing domain
  separation, signature validation rules, account debit/credit
  invariants).
- **`crates/core/tests/settlement.rs`** and **`properties.rs`** exercise
  `settle_batch` end-to-end against constructed `BatchInput`s.
- **`crates/host/tests/batch_builder.rs`** tests `BatchBuilder`'s
  pre-validation against the fixtures in `zk-clob-test-utils`.
- **`crates/host/tests/guest.rs`** runs the fixtures through the actual SP1
  executor (`make test.guest`) and, gated behind `--ignored` since it's
  slow, through real Groth16 proving and verification (`make
  test.guest.real`).
- **`contracts/test/`** (Foundry) covers `ZkClob.sol` and
  `PatriciaProof.sol` against a mock SP1 verifier.

## Known limitations

See [TODO.md](TODO.md) for a running list (no persistent order book across
batches, no order cancellation, single fixed fee, no escape-mode
recovery, etc). The system is scoped to demonstrate provable settlement,
not to be a production exchange.
