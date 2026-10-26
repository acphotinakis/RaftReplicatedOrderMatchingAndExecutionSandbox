# Execution Model

> [!IMPORTANT]
> **Status: proposed design. Not implemented.** This document specifies the
> replicated application: the request path, validation, deduplication,
> matching, accounting, canonical digests, and client retry. Lifecycle terms and
> guarantees `G1`–`G5` are defined in
> [architecture.md §3–§4](architecture.md#3-vocabulary). The decisions it adopts
> are `D-02`, `D-03`, `D-08`, `D-09`, `D-10`, `D-11`, and `D-12`
> ([architecture.md §12](architecture.md#12-decision-register)). Raft behavior
> is specified in [raft-design.md](raft-design.md).

The model reconciles two inputs. The charter and scope documents require a
deterministic **matching** engine with price-time priority. The MVP note
([scoping/mvp.md](scoping/mvp.md)) uses OHLCV bars with next-bar-open fills. The
reconciliation is a single-instrument central limit order book. Committed
market quotes supply external liquidity, and bars are converted to quotes before
they enter the log ([§5.3](#53-external-liquidity-from-market-events)).

## 1. Request lifecycle

The path of one `SubmitOrder` request with idempotency key `(c, s)`:

| Step | Where | What happens | Resulting state | If it fails here |
|---|---|---|---|---|
| 1. Submission | Client library | Encodes `Request{c, s, command}` canonically ([§9.1](#91-encoding-rules)) and sends it to the presumed leader. | — | Timeout or connection error: retry the same bytes ([§10](#10-client-responses-and-retry)). |
| 2. Admission | Leader client endpoint | Rejects non-leaders with `NotLeader`. Runs **stateless** validation ([§3.1](#31-stateless-validation-admission-and-apply)). | — | `Rejected{stage: Admission}`. Nothing is logged and `s` is **not** consumed. |
| 3. Append | Leader Raft core | Appends entry `(i, t, Command, payload)` and records `pending[i]`. | written, then synchronized (leader) | Leader crash: the entry may vanish. The client gets no reply and retries (`F02`–`F04`). |
| 4. Replication | Followers | Each follower synchronizes the entry, then replies success ([raft-design.md §8.7](raft-design.md#87-receiving-appendentries-and-conflict-repair)). | replicated (per follower) | Lost messages are re-sent on the next heartbeat (`F10`). |
| 5. Commitment | Leader | Majority rule with the current-term restriction ([raft-design.md §8.6](raft-design.md#86-commitment)). | committed | Leader crash: a later leader still holds the entry (Leader Completeness) and commits it after its `Noop` (`F05`). |
| 6. Application | Every node, in index order | Decode → deduplicate ([§4](#4-replicated-request-deduplication)) → **stateful** validation ([§3.2](#32-stateful-validation-apply-only)) → match ([§5](#5-deterministic-matching)) → settle ([§7](#7-portfolio-updates)) → record the outcome in the session table. | applied (per node) | `apply` is total and cannot fail. A crash loses only volatile state, which is rebuilt by replay. |
| 7. Response | Leader only | Sends the outcome for `pending[i]` if `log[i].term` still matches. | acknowledged | Reply lost or leader crashed: the client retries and receives the cached outcome (`F06`, `F07`; [§10.3](#103-lost-response-after-commitment)). |
| 8. Completion | Client library | An apply-stage outcome advances the client's next sequence to `s + 1`. | — | — |

Followers apply in step 6 when they learn the new `commit_index`. They never
respond to clients.

## 2. Commands and the request envelope

```text
Request   = { schema_version: u16, client_id: u64, request_sequence: u64, command: Command }

Command   = Genesis {
              instrument: bytes,            -- e.g. "SPY"
              price_scale_exp: i8,          -- -4  => 1 tick = 0.0001 USD   (D-08)
              accounts: [ { client_id: u64, cash: i64, position: i64 } ],
              market_publisher: u64,        -- only client allowed to send MarketEvent
              external_account: u64,        -- counterparty for market liquidity
              max_price: i64, max_quantity: i64 }
          | MarketEvent {
              trace_seq: u64,               -- strictly increasing trace position
              event_ts_ns: i64,             -- trace timestamp: data only, never used for ordering
              body: Quote { bid_px: i64, bid_qty: i64, ask_px: i64, ask_qty: i64 }
                  | Trade { px: i64, qty: i64 } }
          | SubmitOrder { side: Buy | Sell, limit_px: i64, qty: i64 }   -- GTC limit (D-11)
          | CancelOrder { order_id }                                    -- DEFERRED
```

- `Genesis` must be the first applied command (`Noop` entries may precede it).
  It is the **only** source of initial balances and parameters (`D-12`). The
  replay driver submits it, with the $100,000 default from
  [scoping/mvp.md](scoping/mvp.md) set as `cash = 1_000_000_000` units.
- A `Quote` side with quantity 0 is absent. A quote with both quantities 0
  clears external liquidity.
- There is no `Bar` command. Bars are converted upstream ([§5.3](#53-external-liquidity-from-market-events)).
- Fills are **derived** inside `apply`. They are never submitted as commands, so
  no independently generated fill can be duplicated
  ([api_combinations.md §9](scoping/ideas/api_combinations.md)).
- `command_id: String` in [scoping/mvp.md](scoping/mvp.md) is replaced by the
  idempotency key `(client_id, request_sequence)` from the scope documents.

## 3. Validation

### 3.1 Stateless validation (admission and apply)

These checks depend only on the request bytes. The leader runs them at admission
as a fast path, and `apply` runs them again because admission is not replicated.

| Check | Rejection |
|---|---|
| Payload decodes, enum tags are valid, no trailing bytes | `Malformed` |
| `schema_version` is supported | `UnsupportedSchema` |
| `qty > 0`; `limit_px > 0` (orders) | `InvalidQuantity` / `InvalidPrice` |
| Quote: sizes `≥ 0`; price `> 0` on each present side; `bid_px < ask_px` when both are present | `InvalidQuantity` / `InvalidPrice` / `CrossedQuote` |

### 3.2 Stateful validation (apply only)

These checks depend on replicated state, so only `apply` can evaluate them
correctly. A leader's view at admission may lag the committed state, or the
leader may already be deposed.

| Check | Rejection |
|---|---|
| `Genesis` already applied (for non-`Genesis`), or not applied (for `Genesis`) | `NotInitialized` / `AlreadyInitialized` |
| `SubmitOrder` from a declared account; `MarketEvent` only from `market_publisher` | `UnknownAccount` / `Unauthorized` |
| `limit_px ≤ max_price`, `qty ≤ max_quantity` | `InvalidPrice` / `InvalidQuantity` |
| Buy: `cash − reserved_cash ≥ limit_px × qty` | `InsufficientCash` |
| Sell: `position − reserved_qty ≥ qty` (no short selling) | `InsufficientPosition` |
| `trace_seq > last_trace_seq` | `MarketSequenceRegression` |
| All arithmetic in the planned effect is in range ([§6](#6-integer-arithmetic)) | `ArithmeticOverflow` |

An apply-stage rejection **consumes** the sequence number. Its outcome is cached
like any other, so a retry returns the same rejection. `Unauthorized` is a
determinism guard against misrouted commands. It is not a security mechanism
([limitations.md §2](limitations.md#2-explicit-non-goals)).

## 4. Replicated request deduplication

The session table maps `client_id → { last_seq, payload_digest, outcome }`. A
client with no entry has `last_seq = 0`. It is replicated state: covered by
digest `D_sessions`, rebuilt by replay after restart, and required in any future
snapshot.

When `apply` processes request `(c, s, payload)` with `d = SHA-256(payload)`:

| Condition | State change | Outcome returned |
|---|---|---|
| `s == last_seq + 1` | Execute (validate, match, settle). Set the session to `{s, d, outcome}`. | `outcome` (`duplicate = false`) |
| `s == last_seq` and `d == payload_digest` | **none** | cached `outcome` (`duplicate = true`) |
| `s == last_seq` and `d ≠ payload_digest` | none | `Rejected(KeyConflict)` (not cached) |
| `s < last_seq` | none | `Rejected(StaleSequence)` (not cached) |
| `s > last_seq + 1` | none | `Rejected(SequenceGap)` (not cached) |

**Why deduplicate at apply rather than at the leader.** The same key can appear
in the log more than once: the original under one leader, and the retry under
another. Log order is the only order the replicas agree on. Only `apply` sees
both copies in that order. A leader-side check could miss a copy that is
committed but not yet applied locally, or that sits in another node's
uncommitted suffix.

**Effectively-once argument (`G4`).** Execution happens only in the first row,
which requires `s == last_seq + 1` and then sets `last_seq = s`. `last_seq` never
decreases, so key `(c, s)` can never execute again. By `G1` and `G2`, every node
executes it at the same index with the same result. A client that keeps retrying
the same bytes eventually receives either the first-row outcome or the
second-row cached copy of it, provided `G5`'s assumptions hold. This guarantees
one logical effect per key. It does **not** mean the request was delivered or
logged only once.

**Client obligations (`D-09`):** at most one outstanding request per
`client_id`; byte-identical retries; and the sequence advances only after an
apply-stage outcome. Because of the one-outstanding rule, retaining only the
latest outcome is sufficient, and `StaleSequence` should never reach a correct
client. Answering retries directly from the session table at the leader is a
deferred optimization ([§10.2](#102-leader-behavior)).

## 5. Deterministic matching

### 5.1 Determinism rules for `apply`

`apply` must not depend on, call, or contain:

- wall-clock or monotonic time, timers, sleeps;
- randomness, including randomized hash seeds whose iteration order is observable;
- floating point;
- node identity, process ID, addresses, pointer values, allocation order;
- unordered container iteration that affects outcomes, encodings, or digests.
  Use ordered maps, or sort explicitly;
- locale, environment variables, files, network, threads, async tasks;
- panics/exceptions for invalid input. Every invalid input becomes a
  deterministic `Rejected` outcome.

`apply` is two-phase. **Plan** computes the complete effect (fills, balance
deltas, status changes) with checked arithmetic and may reject. **Commit** then
performs only infallible mutations. A rejected request leaves the book,
accounts, and orders unchanged. Only the session record and the log-prefix
digest change.

### 5.2 Book and price-time priority

There is one instrument. Each resting order has
`{order_id, account, side, limit_px, orig_qty, remaining_qty, status, priority}`.

- Bids are ordered by `limit_px` **descending**, then `priority` ascending.
- Asks are ordered by `limit_px` **ascending**, then `priority` ascending.
- `priority = (log index, ordinal)` of the entry that placed the order on the
  book. This committed position is the authoritative arrival time. Trace
  timestamps and node clocks are never used.

### 5.3 External liquidity from market events

**`Quote` at index `i`** (applied by every node identically):

1. Expire any resting orders of `external_account` (status `Expired`).
2. If `bid_qty > 0`: create an external buy order `(i, 0)` at `bid_px` for
   `bid_qty` and match it as the aggressor ([§5.4](#54-matching-algorithm)).
   Any remainder rests.
3. If `ask_qty > 0`: create an external sell order `(i, 1)` at `ask_px` for
   `ask_qty`, match it as the aggressor, and rest any remainder.

The bid side is processed before the ask side. Since `bid_px < ask_px`, the
external remainders never cross each other.

**`Trade`** updates only the market reference (`last_trade_px`, `last_trade_qty`).
It does not match in the MVP. Trade-driven fill models are deferred.

**Bars (upstream conversion, `D-02`).** For bar `k`, the trace builder emits:

1. `Quote{bid = open_k − h, ask = open_k + h, sizes = z}`
2. `Quote{0, 0, 0, 0}` (clear), after which the strategy observes bar `k`'s close.

The half-spread `h` and size `z` are trace-builder parameters, recorded in the
trace manifest. A strategy order submitted after bar `k` closes finds no
external liquidity, so it rests. It fills only when bar `k+1`'s opening quote is
applied. This reproduces the MVP's "execute at the next eligible bar's open"
inside the order book, with no lookahead and no fill against stale prices.

### 5.4 Matching algorithm

For an aggressor `o` (a buy shown; a sell is symmetric) at index `i`:

```text
k := 0
while o.remaining > 0:
    r := best resting ask                         -- lowest price, then lowest priority
    if r is none or r.limit_px > o.limit_px: break
    q  := min(o.remaining, r.remaining)
    px := r.limit_px                              -- resting order's price
    emit Fill{ fill_id: (i, k), buy: o.id, sell: r.id, px, q }; k := k + 1
    settle(fill)                                  -- §7, inside the same commit
    o.remaining -= q; r.remaining -= q
    if r.remaining == 0: r.status := Filled; remove r
if o.remaining == 0: o.status := Filled
else: rest o with priority (i, ordinal); o.status := (o had fills ? PartiallyFilled : Open)
```

- Partial fills are inherent to a book and are allowed. This diverges from
  [scoping/mvp.md](scoping/mvp.md)'s "no partial fills".
- **Self-matches** (aggressor and resting order in the same account) are allowed
  and recorded. Self-trade prevention is deferred.
- After every `apply`, best bid < best ask (A6).

### 5.5 Order statuses

`Open` and `PartiallyFilled` (resting) can move to `Filled` (terminal). External
orders can also move to `Expired` (terminal). `CancelOrder` and `Cancelled` are
deferred. A rejected `SubmitOrder` creates no order. It exists only as a cached
outcome.

## 6. Integer arithmetic

| Quantity | Type | Unit |
|---|---|---|
| Price | `i64` | tick = 10^`price_scale_exp` USD (MVP: 0.0001 USD) |
| Quantity / position | `i64` | lot = 1 share |
| Cash, reservations | `i64` | 0.0001 USD (1 tick × 1 lot) |

- Notional `px × q` is computed with checked multiplication, or widened to
  128-bit and range-checked. Balance updates use checked addition and
  subtraction. Any overflow during **plan** rejects the whole request with
  `ArithmeticOverflow`.
- No division and no rounding happen inside `apply`. Every product of ticks and
  lots is an exact integer.
- **Conversion from decimal** happens only in the trace builder. It parses the
  textual decimal exactly, without going through binary floating point. Most
  JSON libraries parse numbers as `f64` by default, so the builder must use a
  raw-number or decimal mode. A value that is not exactly representable at the
  configured scale (e.g., `vw = 471.001212`) is not used in replicated commands.
  If such a value is required, the record is rejected and the rejection is
  logged in the manifest.
- Dollar display is a reporting concern only.

## 7. Portfolio updates

Each account holds `{cash, position, reserved_cash, reserved_qty}`. Client
accounts start from `Genesis`. The external account starts at zero and is
unconstrained, so it may go negative. It has no reservations.

| Event | Effect |
|---|---|
| Accept a buy `(L, Q)` from client account `a` | `a.reserved_cash += L × Q` |
| Accept a sell `(L, Q)` from client account `a` | `a.reserved_qty += Q` |
| Fill `(px, q)`: buyer `B` with order limit `L_B`, seller `S` | `B.cash −= px×q`; `B.position += q`; if `B` is a client: `B.reserved_cash −= L_B×q`. `S.cash += px×q`; `S.position −= q`; if `S` is a client: `S.reserved_qty −= q` |
| External order expires | none (no reservations) |

A fill updates both orders and both accounts in one commit step. No state is
observable between them.

### Application invariants (checked after every `apply` in tests)

| ID | Invariant |
|---|---|
| A1 | Total cash over all accounts, external included, equals the `Genesis` total. |
| A2 | Total position over all accounts, external included, equals the `Genesis` total. |
| A3 | Client accounts satisfy `cash ≥ 0`, `position ≥ 0`, `0 ≤ reserved_cash ≤ cash`, and `0 ≤ reserved_qty ≤ position`. |
| A4 | `reserved_cash(a) = Σ limit_px × remaining` over `a`'s resting buys, and `reserved_qty(a) = Σ remaining` over its resting sells. |
| A5 | For every order, `0 ≤ remaining ≤ orig`, and `Σ fill qty = orig − remaining`. There are no overfills. |
| A6 | The resting book is never crossed. Every resting order has `remaining > 0` and a resting status. |
| A7 | Each fill's `px` satisfies `seller limit ≤ px ≤ buyer limit`. |
| A8 | Session `last_seq` per client never decreases. Each key executes at most once (`G4`). |

## 8. Logical time and identifiers

- **Authoritative order** is the log index, plus an ordinal within an entry.
- `OrderId = (index, ordinal)`. A client order uses ordinal 0. External bid and
  ask orders use ordinals 0 and 1 of their `Quote`'s index. `FillId = (index, k)`.
- `event_ts_ns` and `trace_seq` are stored as market reference data. They never
  drive ordering, expiry, or matching.
- Identifiers derived from indexes are identical across replicas (`G1`). They may
  differ across *runs* if the log contains different `Noop` or retry entries.
  Cross-run comparison therefore uses `D_econ` ([§9.2](#92-component-digests)).

## 9. Canonical state encoding and digests

### 9.1 Encoding rules

The same rules apply to request payloads and to digest inputs:

- Fixed-width big-endian integers. Booleans and enum tags are one byte. Byte
  strings carry a `u32` length prefix.
- Struct fields appear in schema-defined order, and every field is always
  present. An optional field is encoded as a tag (0 or 1) followed by its value.
- A sequence is encoded as a `u32` count followed by its elements in the defined
  order. A map is encoded as a sequence of pairs sorted by the canonical bytes of
  the key.
- Never hash in-memory struct bytes, padding, pointers, transport-encoded
  messages (Protobuf/serde output is not canonical), or incidental iteration
  order.
- Each digest is `SHA-256(domain_label ‖ schema_version ‖ encoding)`, where the
  label is, for example, `"rromes/v1/book"`.

### 9.2 Component digests

| Digest | Covers | Ordering |
|---|---|---|
| `D_log` | Committed-prefix hash chain: `P_0 = 0^32`, `P_i = SHA-256("rromes/v1/log" ‖ P_{i−1} ‖ index ‖ term ‖ kind ‖ payload)` | by index |
| `D_book` | Resting orders | bids (price ↓, priority ↑), then asks (price ↑, priority ↑) |
| `D_orders` | Every accepted order, including terminal ones | by `order_id` |
| `D_fills` | Every fill | by `fill_id` |
| `D_accounts` | All accounts, external included | by account ID |
| `D_sessions` | Session table | by `client_id` |
| `D_market` | `Genesis` parameters, last quote, last trade, `last_trace_seq` | fixed |
| `D_state` | `SHA-256` of `D_book … D_market` (excludes `D_log`) | fixed |
| `D_econ` | **Cross-run projection:** accounts plus, for each idempotency key in key order, the outcome kind and its fills as `(counterparty kind, px, q)`, with log-derived IDs removed | by key |

`G2` is evaluated on `D_log` and `D_state` at each common applied index
([experiments.md §7.2](experiments.md#72-comparison-at-common-applied-indexes)).
`D_econ` is used only for run-to-run repeatability of lockstep replays.
Recomputing digests costs O(state) per index. Correctness runs compute them at
every index. Performance runs compute them only at checkpoints, so digest cost
is not mistaken for consensus cost.

## 10. Client responses and retry

### 10.1 Reply types

| Reply | Definitive? | Client action |
|---|---|---|
| `Outcome{stage: Apply, duplicate}` | yes | Return the outcome. `next_seq = s + 1`. |
| `Outcome{stage: Admission, Rejected}` | yes, for this payload | Return the rejection. `next_seq` is unchanged. |
| `NotLeader(hint)` | no | Retry the same bytes at `hint`, or the next node. |
| `Retryable(reason)` | no | Back off, then retry the same bytes. |
| Timeout / connection error | no | Back off, then retry the same bytes at another node. |

Client loop: `submit` keeps the identical payload until it receives a definitive
reply. Retries use bounded exponential backoff with seeded jitter. In
correctness experiments there is no retry limit, and every attempt is logged.

### 10.2 Leader behavior

The leader answers only from `apply` results for indexes it proposed in its
current term ([raft-design.md §8.9](raft-design.md#89-client-proposals-and-pending-responses)).
A retry is appended as a new entry, even if the original is already applied.
Answering from the session table without appending would also be safe, because
the table reflects only committed state. That optimization is deferred so the
MVP has one response path.

### 10.3 Lost response after commitment

```mermaid
sequenceDiagram
    participant C as Client c
    participant N1 as n1 (leader, term 3)
    participant N2 as n2
    participant N3 as n3
    C->>N1: Submit(c, 7, BUY 10 @ L)
    N1->>N1: append index 41 (term 3), synchronize
    N1->>N2: AppendEntries(prev=40, [41])
    N1->>N3: AppendEntries(prev=40, [41])
    N2->>N2: synchronize index 41
    N2-->>N1: success, match_index=41
    Note over N1: 41 committed (n1 + n2 synchronized)
    N1->>N1: apply 41: execute (c,7), session[c]=7, outcome O
    N1--xC: Outcome O (lost: crash or dropped reply)
    Note over N1: n1 crashes (F06) or reply dropped (F07)
    Note over C: timeout - result unknown, keep key (c,7)
    Note over N2,N3: n2 wins term 4 (n2 holds 41, so n3 cannot win without n2)
    N2->>N2: append Noop index 42 (term 4), synchronize
    N2->>N3: AppendEntries(prev=40, [41, 42])
    N3->>N3: synchronize 41..42
    N3-->>N2: success, match_index=42
    Note over N2: 42 committed in term 4, so 41 committed indirectly
    N2->>N2: apply 41 (executes (c,7), outcome O), apply 42 (Noop)
    C->>N2: Submit(c, 7, identical bytes) [retry]
    N2->>N2: append index 43, replicate to n3, commit
    N2->>N2: apply 43: session[c].last_seq=7 and digest equal, no state change
    N2-->>C: Outcome O (duplicate=true)
    Note over C: next_seq = 8; exactly one order and one set of fills exist
```

The same key still produces one effect in the neighboring cases:

- **Not committed before the crash** (`F03`, `F04`): the new leader may lack index
  41. The retry is then the first copy to reach `apply` and executes once. If
  41 survives on a node and is later committed, the second copy is a duplicate.
- **Retry reaches the new leader before 41 is applied there:** the retry is
  appended after 41, and index order guarantees 41 executes first.
- **Restarted n1:** it rejoins as a follower, keeps its matching entry 41,
  re-applies 41–43 from an empty state, and produces the same digests at every
  common applied index (`G2`).

## 11. Replay driver and strategy

**Lockstep mode** (all correctness experiments) has one driver process with two
client IDs:

```text
submit Genesis; await outcome
for each trace event e_k in order:
    submit MarketEvent(e_k) as market_publisher; await apply-stage outcome
        -- its outcome lists fills it caused, so the strategy learns of resting-order fills
    intents := strategy.on_event(e_k)             -- sees only e_1..e_k
    for each intent: submit SubmitOrder as strategy client; await outcome
```

This fixes the logical command order independently of timing. Runs with the
same trace and parameters therefore yield equal `D_econ`, even with faults. It
also enforces the no-lookahead rule from [scoping/mvp.md](scoping/mvp.md) §4.

**Free-running mode** (performance experiments): publisher and clients submit
independently, so log interleaving depends on timing. Replicas must still agree
(`G1`, `G2`). Run-to-run equality is not expected.

**MVP strategy:** 5/20 SMA crossover on bar closes, 10 shares, no short selling
([scoping/mvp.md](scoping/mvp.md)). The comparison uses integers
(`20 × Σ5 > 5 × Σ20`), with no division or floating point. Orders are marketable
limits at `close ± collar`. The collar is recorded in the run metadata. An
order that does not fill on the next open rests (GTC). Without `CancelOrder`,
its reservation persists. This is an accepted MVP limitation
([limitations.md §4.5](limitations.md#45-market-and-execution-realism)).

The strategy runs outside replicated state. Its determinism matters for run
repeatability, not for replica agreement.

## 12. Excluded from the MVP

`CancelOrder`, IOC/market orders, multiple instruments, fees, margin, short
selling, self-trade prevention, trade-driven or depth-aware fill models,
asynchronous fill streaming to clients, client read APIs, session expiry, and
windowed per-client pipelining. See [architecture.md §12](architecture.md#12-decision-register)
for the decisions and [limitations.md §4.6](limitations.md#46-deferred-functionality-and-architectural-consequences)
for their consequences.
