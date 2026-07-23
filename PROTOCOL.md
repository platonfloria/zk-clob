# ZK Exchange Protocol

## Batch Authentication

Every batch is bound to one specific deployment and one specific prior
state, so a proof cannot be replayed elsewhere or reordered:

-   `SigningDomain{protocolVersion, chainId, exchangeId}` is hashed into
    every order/withdrawal signature and into the batch itself; the
    contract rejects a batch whose domain doesn't match its own protocol
    version, chain ID, and address.
-   `configHash` is pinned immutably at contract deployment. A batch is
    rejected unless its committed `configHash` matches — assets, markets,
    and fees cannot change without deploying a new contract.
-   `batchId` is a monotonic counter, checked independently of the state
    root (`WrongBatchId`), so batches must be settled in order.
-   `oldStateRoot` must equal the contract's current `stateRoot`
    (`StaleStateRoot`) — this is what actually prevents replay: once a
    batch settles, its proof's `oldStateRoot` is no longer current.

------------------------------------------------------------------------

## Order Submission and Matching Procedure

1.  User signs an `Order` (market, side, price, quantity, nonce) off-chain
    and sends it to the operator. Signing uses a domain-separated SHA-256
    digest recovered to a secp256k1 signer, not on-chain calldata.
2.  Operator assigns the order a per-batch `sequence` number (arrival
    order, used only to break same-price ties) and slots it into that
    market's order book, sorted by price-time priority.
3.  Operator includes the order in the next `BatchInput`, grouped per
    market into buy/sell index lists.
4.  SP1 Guest:
    -   recovers each order's signer and rejects unknown accounts,
        unknown markets, zero price/quantity, or an invalid signature;
    -   rejects a nonce that isn't exactly the account's next expected
        value — orders and withdrawals from the same account share one
        strictly sequential nonce space, so a withdrawal and an order
        for the same account cannot both claim the same nonce;
    -   re-derives each market's book from the raw order list and
        rejects it unless it is strictly sorted by price-time priority,
        every order is on the declared market/side, and every order
        index is used exactly once — the host-supplied ordering is
        never trusted as-is;
    -   matches buys against sells at the resting order's price (the
        side with the lower sequence number) until one side is
        exhausted or the best bid drops below the best ask, splitting
        partially-filled orders across trades;
    -   rejects a match where both sides belong to the same account
        (self-trade);
    -   debits the buyer the trade value plus a fee (a single fixed
        `buyerFeeBps`, charged in the market's quote asset to the buyer
        only), credits the seller the trade value, credits the base
        asset to the buyer and quote asset to the seller, and credits
        the fee to the exchange's configured fee recipient;
    -   computes the new `state_root`.
5.  Guest commits `tradesHash` (a hash of every resulting trade) alongside
    the new state root; the contract does not interpret trades itself —
    it only re-derives account balances via the state root.

------------------------------------------------------------------------

## Deposit Procedure

1.  User calls `deposit()` (native ETH, `payable`, no token argument) or
    `deposit(token, amount)` (ERC-20) on the contract. For ERC-20, the
    contract measures its own balance before and after `transferFrom`
    and reverts if the received amount doesn't exactly match `amount`
    (fee-on-transfer tokens are rejected, not silently short-credited).
    Zero-amount deposits and amounts exceeding `uint128::MAX` are
    rejected up front.
2.  Contract locks the assets.
3.  Contract appends a new deposit message to the on-chain deposit queue
    and assigns a sequential `depositId`.
4.  Operator periodically reads pending deposits starting from
    `nextUnprocessedDeposit`.
5.  Operator includes a prefix of pending deposits in the next
    `BatchInput`.
6.  SP1 Guest:
    -   verifies batch structure;
    -   verifies the deposit cursor is exactly sequential (no gaps,
        starts at `old_deposit_cursor`);
    -   creates new accounts if necessary;
    -   credits deposited assets;
    -   advances the deposit cursor;
    -   computes the new `state_root`;
    -   commits:
        -   `old_state_root`
        -   `new_state_root`
        -   `old_deposit_cursor`
        -   `new_deposit_cursor`
        -   `consumed_deposits_hash`
7.  Settlement contract:
    -   verifies the SP1 proof;
    -   verifies the new cursor doesn't advance past the contract's own
        `nextDepositId` — a batch cannot claim to consume deposits that
        haven't been queued yet;
    -   recomputes the hash of consumed deposits from its own queue over
        the claimed cursor range;
    -   checks it matches `consumed_deposits_hash`;
    -   advances the deposit cursor;
    -   stores the new `state_root`.

------------------------------------------------------------------------

## Normal Withdrawal Procedure

1.  User signs a withdrawal request (asset, amount, recipient, nonce)
    off-chain, the same domain-separated signature scheme as orders.
2.  Operator includes the withdrawal request in the next batch.
3.  SP1 Guest:
    -   verifies signature;
    -   verifies the nonce is exactly the account's next expected value
        (shared sequence with that account's orders, see above);
    -   verifies sufficient balance;
    -   deducts funds from the user's internal balance;
    -   computes the new `state_root`.
4.  Guest commits:
    -   `old_state_root`
    -   `new_state_root`
    -   `withdrawalsHash` — a hash of the canonical executed-withdrawal
        list, not the transfer instructions themselves.
5.  Operator calls `settle()`, passing the proof and public values
    *alongside* the plaintext `Withdrawal[]` array as calldata — the
    proof only commits a hash of this list, it does not carry the
    transfer instructions on-chain.
6.  Settlement contract:
    -   verifies the SP1 proof;
    -   checks `old_state_root` equals the current root;
    -   hashes the calldata `Withdrawal[]` array and checks it matches
        the proof's committed `withdrawalsHash` — this is what ties the
        plaintext transfer instructions to the proven state transition;
    -   updates the state root;
    -   attempts to transfer assets directly to each recipient (native
        ETH via a raw call, ERC-20 via a non-reverting transfer). A
        failed transfer does not revert the batch — the amount is
        credited to `pendingWithdrawals[recipient][asset]` instead, and
        anyone may retry the same transfer later via `withdrawPending`.
        This keeps one unreceivable recipient (e.g. a contract that
        reverts on receiving ETH) from blocking every other withdrawal
        and trade bundled in the same batch.
7.  Replay is prevented because the same proof references the previous
    state root, which is no longer current after settlement (see Batch
    Authentication above).

------------------------------------------------------------------------

## Forced Withdrawal Procedure

1.  User submits a forced withdrawal request (`asset`, `amount`) directly
    to the contract via `requestForcedWithdrawal`. Any account may
    request any asset — including one the exchange doesn't configure —
    since the queue must stay strictly FIFO and validating the asset here
    would let one bad request permanently block every request behind it.
2.  Contract appends it to the immutable forced-withdrawal queue with a
    deadline of `block.timestamp + FORCED_WITHDRAWAL_DELAY`.
3.  Operator must include pending forced withdrawals in subsequent
    batches, in order, starting from `nextUnprocessedForcedWithdrawal`.
4.  SP1 Guest:
    -   verifies request ordering (cursor exactly sequential);
    -   drains `min(requested_amount, available_balance)` from the
        account — never rejects for insufficient balance. A request
        against an unknown asset or a zero balance simply drains zero and
        settles as a no-op, rather than failing the batch;
    -   advances the forced-withdrawal cursor;
    -   computes a new `state_root`.
5.  Guest commits:
    -   `old_state_root`
    -   `new_state_root`
    -   old/new forced-withdrawal cursors
    -   `consumedForcedWithdrawalsHash` (hash of the requests consumed
        from the contract's own queue)
    -   `forcedWithdrawalsHash` (hash of the amounts actually drained,
        passed as calldata alongside the proof, the same
        pass-plaintext-and-check-the-hash pattern as normal withdrawals).
6.  Settlement contract:
    -   verifies the proof;
    -   verifies the cursor advancement doesn't exceed its own
        `nextForcedWithdrawalId`;
    -   recomputes and checks both the consumed-requests hash and the
        drained-amounts hash;
    -   attempts to transfer the drained amount to each user (skipping
        accounts drained for zero); exactly as with normal withdrawals, a
        failed transfer is credited to `pendingWithdrawals` rather than
        reverting the batch, so a single unreceivable account can't block
        the cursor — and therefore can't force escape mode — behind it;
    -   updates the state root.
7.  If the *oldest* unprocessed forced withdrawal is not processed before
    its own deadline:
    -   `settle()` continues to work normally until someone acts;
    -   anyone may call `activateEscapeMode()`, which checks only that
        oldest request's deadline has elapsed.
8.  Once escape mode is active:
    -   `stateRoot` is frozen at its last settled value; `settle()`
        reverts unconditionally from then on;
    -   each account may call `escapeWithdraw` **once** (enforced by an
        `escaped` mapping), presenting its own account leaf (balances +
        next nonce) and a single-leaf Patricia multiproof
        (`PatriciaProof.sol`) against the frozen root;
    -   the contract reconstructs the root itself from the supplied leaf
        and proof and pays out every balance in that leaf directly if it
        matches — the call is permissionless, since funds only ever move
        to the account named in the leaf, so anyone may submit the proof
        on a stuck account's behalf;
    -   escape mode is **permanent**: there is currently no procedure to
        return to normal settlement once activated (see TODO.md).

------------------------------------------------------------------------

## Conservation Invariant

Independent of the per-operation balance checks above, the guest tracks
total value per asset across the whole batch: starting balances plus
deposits, minus withdrawals, minus forced withdrawals, must equal ending
balances exactly, for every asset touched. A mismatch aborts the entire
batch — this is a global safety net against a bug in trade or fee
settlement that balances individual debits/credits but still lets value
drift in aggregate.
