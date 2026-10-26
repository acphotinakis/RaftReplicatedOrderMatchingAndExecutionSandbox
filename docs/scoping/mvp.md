# Base MVP: Raft-Replicated Order Execution Sandbox

The MVP is a three-node Raft cluster implemented from scratch in Rust that replays historical SPY OHLCV data, generates simulated trading orders, and guarantees identical committed order, fill, and portfolio state across all three replicas—even after a leader crashes and recovers.

We should prioritize proving distributed consensus correctness before introducing complex trading strategies, historical quotes, or order-book matching.

## 1. MVP architecture

Alpaca Historical Bars API

SPY · 1-minute OHLCV · One trading session

Deterministic Replay + Strategy

Processes bars and generates simulated orders

Custom Three-Node Raft Cluster

Node A

LeaderRaft log

Node B

FollowerRaft log

Node C

FollowerRaft log

RequestVote · AppendEntries · Majority commitment

Independent Execution State Machine on Each Node

Orders → Fills → Cash / SPY Position → SHA-256 Hash

Fault Injection & Validation

Leader crash · New election · Recovery · State comparison

## 2. Exact MVP requirements

These are the minimum components I would implement.

| Component         | MVP implementation                              |
| ----------------- | ----------------------------------------------- |
| Language          | Rust                                            |
| Cluster           | 3 independent processes on localhost            |
| Raft              | Implement from scratch                          |
| Market data       | Alpaca historical SPY 1-minute OHLCV            |
| Dataset           | One complete regular trading session            |
| Strategy          | Simple moving-average crossover                 |
| Order types       | Market BUY and SELL                             |
| Execution         | Next eligible bar's open                        |
| Portfolio         | Cash balance and SPY share position             |
| Persistence       | Durable Raft term, vote, and log                |
| Failure injection | Kill the leader during replay                   |
| Recovery          | Restart leader and synchronize                  |
| Validation        | Compare committed logs, fills, and state hashes |

No actual Alpaca trading orders are submitted.

## 3. Minimal Raft implementation

You need these features to claim a functioning Raft MVP.

Leader election

Followers become candidates after randomized election timeouts. Candidates request votes and become leaders after receiving a majority. Include term tracking, log up-to-date checks, and step-down on higher terms.

Heartbeats

Leaders send periodic empty `AppendEntries` RPCs. Followers reset their election timers when receiving valid leader communication.

Log replication

Replicate commands using `AppendEntries` , previous-index/term consistency checks, and per-follower `nextIndex` / `matchIndex` tracking.

Majority commitment

Commit current-term entries when stored on a majority. Apply committed entries in order, exactly once per replica.

Crash recovery

Restore persisted Raft state, reconcile divergent uncommitted log suffixes, and rebuild application state from committed commands.

For the MVP, you do not need membership changes, dynamic cluster configuration, log compaction, or snapshots. Use a fixed three-node cluster and retain the entire log.

## 4. Minimal trading strategy and execution model

Keep the strategy intentionally simple.

Use a 5-bar and 20-bar simple moving average:

* BUY 10 shares when the 5-bar SMA crosses above the 20-bar SMA.
* SELL 10 shares when it crosses below, provided sufficient shares are held.
* Start with $100, 000 in simulated cash.
* No short selling, margin, commissions, or partial fills.
* Execute at the next eligible bar's open.

The replicated state machine needs only a small command set:

```
enum Command {
    MarketBar {
        sequence: u64,
        timestamp_ns: i64,
        open: i64,
        high: i64,
        low: i64,
        close: i64,
        volume: u64,
    },
    SubmitOrder {
        command_id: String,
        side: Side,
        quantity: u64,
    },
}
```

The replay controller must ensure that the next bar's open is not revealed to the strategy before its decision. All market events and order submissions go through the same Raft log, so every replica sees the same execution sequence.

For example, a signal generated after bar 20 closes can submit an order that executes when bar 21 is committed.

A small deterministic state machine is sufficient:

```
struct TradingState {
    cash_cents: i64,
    spy_shares: u64,
    last_bar_sequence: u64,
    orders: Vec<Order>,
    fills: Vec<Fill>,
}
```

The state machine should reject duplicate command IDs and use integer arithmetic with explicit rounding rules.

## 5. Example of a successful MVP demonstration

Demonstration: Crash Node A during replay

1. Start three nodes

   A becomes leader; B and C become followers.
2. Replay historical SPY bars

   Strategy generates BUY and SELL commands. All nodes apply committed entries.
3. Kill Node A

   B and C retain a majority and elect a new leader.
4. Continue replay

   The new leader processes remaining market events and orders.
5. Restart Node A

   It loads its persistent log and catches up with the current leader.
6. Verify state

   Once all nodes have applied the same committed index, compare order history, fills, cash, position, and state hashes.

A successful demonstration might produce output like:

```
[RAFT] Node A elected leader (term 1)
[RAFT] Log index 42 committed by majority
[TRADE] BUY 10 SPY @ 670.25

[FAULT] Node A terminated
[RAFT] Node B elected leader (term 2)
[RAFT] Log index 43 committed by majority
[TRADE] SELL 10 SPY @ 671.10

[RECOVERY] Node A restarted
[RECOVERY] Node A caught up through index 390

[VERIFY] Node A hash: 8f32...a91c
[VERIFY] Node B hash: 8f32...a91c
[VERIFY] Node C hash: 8f32...a91c

[RESULT] PASS — All replicas consistent
```

These are illustrative logs, not actual execution results.

## 6. Suggested implementation milestones

MVP build checklist

0/7 complete

1\. Project setup

Create Rust workspace, configuration, three-node launcher and structured logging.

2\. Raft election

Implement terms, votes, election timeouts, RequestVote and leader heartbeats.

3\. Raft replication

Implement persistent log, AppendEntries, conflict repair and majority commit.

4\. Raft recovery

Restart crashed processes and reconstruct committed state.

5\. Alpaca ingestion

Fetch SPY bars, normalize prices and save a fixed historical trace.

6\. Trading state machine

Implement strategy, orders, next-bar fills, portfolio and deduplication.

7\. End-to-end testing

Inject leader crash, continue replay, restart node and compare state hashes.

## 7. What we explicitly exclude

Do not build these features until the MVP works:

* Historical trades and quotes ingestion.
* Multi-symbol or cross-sectional strategies.
* Full limit-order book and price-time priority matching.
* Partial fills and sophisticated slippage.
* Distributed sharding or multi-Raft.
* Dynamic cluster membership.
* Snapshots and log compaction.
* Live market-data streaming.
* Frontend dashboard.
* Real trading execution.

These can all be added later without changing the project's fundamental research question.

## 8. MVP acceptance criteria

The project is complete when all of the following pass:

| Test            | Required result                                                  |
| --------------- | ---------------------------------------------------------------- |
| Startup         | Three nodes elect exactly one leader per term                    |
| Replication     | All committed entries have the same index, term, and command     |
| Crash tolerance | Cluster continues with one node down                             |
| Safety          | No conflicting entries become committed at the same index        |
| Retry handling  | Retried orders do not produce duplicate fills                    |
| Recovery        | Restarted node catches up                                        |
| Determinism     | All replicas produce identical state at equal applied indices    |
| Repeatability   | Re-running the same trace produces identical final trading state |

The safety requirement should be tested under message delays and conflicting uncommitted logs, not just a clean leader shutdown.

## Final scope

## Minimum Viable Project

Three-node, crash-tolerant Raft consensus implemented from scratch in Rust, with historical SPY OHLCV replay, deterministic simulated market-order execution, durable logs, leader-failure recovery, and identical replicated portfolio state.

Primary objective: prove consensus correctness and state-machine consistency under leader failure, not trading profitability or realistic exchange matching.

One naming distinction is worth preserving: with market orders filled from OHLCV prices, this is an order-execution sandbox, not a full order-matching engine. If your professor requires actual matching between opposing buy and sell limit orders, that would need a separate minimal order-book component.
