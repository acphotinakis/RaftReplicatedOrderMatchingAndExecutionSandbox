# Building Raft From Scratch for the Alpaca Trading Sandbox

Since you must implement Raft yourself, I would significantly narrow the initial project scope.

Recommended architecture: Three Rust processes implementing Raft from scratch, a deterministic SPY trading strategy using historical OHLCV data, and a replicated order-execution state machine. Add historical quotes only after the consensus implementation works reliably.

The challenge is no longer primarily the trading simulator. It is implementing a correct distributed consensus protocol.

## 1. What you actually need to build

Divide the project into six components:

| Component        | Responsibility                               | Difficulty |
| ---------------- | -------------------------------------------- | ---------- |
| Alpaca ingestion | Download historical SPY OHLCV                | Low        |
| Replay engine    | Deliver market events in deterministic order | Medium     |
| Strategy         | Generate simulated orders                    | Low        |
| Raft consensus   | Elections, replication, commitment, recovery | Very High  |
| Execution engine | Calculate fills and portfolio state          | Medium     |
| Fault testing    | Crash nodes and verify consistency           | High       |

I would allocate approximately 60% of the implementation effort to Raft, 20% to the deterministic execution engine, and 20% to data ingestion, integration, and testing.

## 2. Simplified system architecture

Alpaca Historical OHLCV

SPY · 1-minute bars · Parquet

Replay Controller + Strategy

Deterministic bar progression → simulated orders

Your Raft Implementation

Node 1

LeaderPersistent log

Node 2

FollowerPersistent log

Node 3

FollowerPersistent log

RequestVote · AppendEntries · Heartbeats · Majority commit

Independent Execution State Machines

Orders · Fills · Cash · Positions · State hashes

Each node is an independent process. The replicas must not share in-memory Raft state, order books, or portfolios.

You can run all three on one MacBook using separate ports, such as `7001` , `7002` , and `7003` .

## 3. The Raft implementation itself

You need to implement five fundamental subsystems.

### A. Node state and persistent storage

Every Raft node has a role and a term.

```
enum Role {
    Follower,
    Candidate,
    Leader,
}

struct LogEntry {
    term: u64,
    index: u64,
    command: Command,
}

struct RaftNode {
    id: u64,
    role: Role,

    // Persistent across crashes
    current_term: u64,
    voted_for: Option<u64>,
    log: Vec<LogEntry>,

    // Volatile
    commit_index: u64,
    last_applied: u64,

    // Leader-only
    next_index: Vec<u64>,
    match_index: Vec<u64>,
}
```

This is a conceptual skeleton. Your implementation also needs peer addresses, timers, RPC handling, storage interfaces, and the execution state machine.

The three persistent fields are particularly important:

* `current_term`
* `voted_for`
* `log`

Changes to these fields must be durably persisted before sending RPC responses that depend on them.

For crash recovery, you must also reconstruct the applied state correctly, using either replay from a known initial state or a consistent snapshot.

### B. Leader election

Implement the `RequestVote` RPC.

```
struct RequestVote {
    term: u64,
    candidate_id: u64,
    last_log_index: u64,
    last_log_term: u64,
}

struct RequestVoteResponse {
    term: u64,
    vote_granted: bool,
}
```

Election sequence:

Simplified election: Node A receives its own vote and Node B's vote, giving it a majority of two.

You need randomized election timeouts to reduce split votes. For a local test cluster, you could initially use election timeouts in the 300–600 ms range and heartbeats around every 75 ms. These are configurable test parameters, not universal Raft requirements.

The candidate must also check whether its log is sufficiently up to date to be eligible for a vote.

### C. Log replication

Implement `AppendEntries` .

```
struct AppendEntries {
    term: u64,
    leader_id: u64,
    prev_log_index: u64,
    prev_log_term: u64,
    entries: Vec<LogEntry>,
    leader_commit: u64,
}

struct AppendEntriesResponse {
    term: u64,
    success: bool,
}
```

When the leader receives a strategy order:

1. Append the order to its local persistent log.
2. Send `AppendEntries` to followers.
3. Wait for successful replication on a majority.
4. Advance `commit_index` according to Raft's commitment rules.
5. Apply the committed command to the execution state machine.
6. Return the result to the client.

For a three-node cluster, a majority is two nodes.

Importantly, a leader directly advances commitment by majority replication only for entries from its current term. Older-term entries become committed indirectly through the commitment of a current-term entry.

### D. Log consistency and conflict repair

Suppose Node A crashes after appending an entry that was never committed.

Before leader failure

| Index | Node A | Node B | Node C |
| ----- | ------ | ------ | ------ |
| 1     | BUY 10 | BUY 10 | BUY 10 |
| 2     | SELL 5 | SELL 5 | SELL 5 |
| 3     | BUY 20 | —      | —      |

Entry 3 is uncommitted and may be discarded.

After a new leader is elected

The new leader can append a different entry at index 3. When Node A recovers, its conflicting uncommitted entry must be replaced.

Raft preserves committed entries, not every entry ever appended by a failed leader.

Implement the log-matching rule: a follower accepts new entries only when the preceding index and term match its local log. Otherwise, the leader retries from an earlier index.

Never overwrite a committed entry.

### E. Crash recovery

When a crashed node restarts:

1. Read its persisted term, vote, and log.
2. Start as a follower.
3. Reconstruct its state machine from a valid snapshot or committed-log replay.
4. Receive missing entries from the current leader.
5. Apply committed entries in order.
6. Verify that its state matches the other replicas at the same applied index.

For your initial bounded replay, you can avoid implementing Raft log compaction and `InstallSnapshot` . Instead, retain the complete log and replay it during recovery. Make this limitation explicit in your report.

## 4. The trading engine should be deliberately simple

I would use a deterministic SPY strategy based on historical 1-minute bars.

For example:

* Compute a 5-period and 20-period simple moving average.
* Submit BUY 10 SPY shares when the short average crosses above the long average.
* Submit SELL 10 shares when it crosses below, provided sufficient shares are held.
* Execute at the next eligible bar's open using a documented fill model.
* Track cash, shares, average cost, and realized P&L.

Use integer fixed-point prices and quantities for deterministic accounting.

The important part is not whether this strategy is profitable. It is that the same committed order sequence produces the same fills and portfolio state on every replica.

### One subtle architectural requirement

If the state machine uses the next bar's open to calculate a fill, every replica must agree on which bar is eligible.

A safe approach is to include market progression in the replicated command stream:

```
enum Command {
    MarketBar {
        sequence: u64,
        open_price: i64,
        close_price: i64,
        volume: u64,
    },
    SubmitOrder {
        command_id: String,
        side: Side,
        quantity: u64,
    },
}
```

You can also include the full OHLCV fields and symbol. The strategy should run outside the replicated state machine, while the committed commands determine the exact execution state.

For the MVP, replicate market bars and orders. With approximately 390 regular-session bars per full SPY trading day, the log is small enough to keep the implementation manageable.

Historical quotes can later be introduced as an alternative market-event command without changing Raft itself.

## 5. What to implement versus what libraries you can still use

Not using a Raft library does not mean writing everything from scratch.

| Functionality          | Recommended approach     |
| ---------------------- | ------------------------ |
| Raft state transitions | Implement yourself       |
| Leader election        | Implement yourself       |
| Log replication        | Implement yourself       |
| Commit advancement     | Implement yourself       |
| Conflict repair        | Implement yourself       |
| Recovery logic         | Implement yourself       |
| TCP networking         | Tokio                    |
| RPC serialization      | serde + JSON or bincode  |
| HTTP Alpaca requests   | reqwest                  |
| Persistence            | SQLite or file-based WAL |
| Logging                | tracing                  |
| Hashing                | SHA-256 library          |

You can use networking, serialization, storage, and hashing libraries without outsourcing the consensus algorithm.

## 6. Suggested repository structure

```
raft-trading-sandbox/
├── src/
│   ├── main.rs
│   ├── raft/
│   │   ├── mod.rs
│   │   ├── node.rs
│   │   ├── election.rs
│   │   ├── replication.rs
│   │   ├── rpc.rs
│   │   ├── log.rs
│   │   ├── storage.rs
│   │   └── timer.rs
│   ├── market/
│   │   ├── alpaca.rs
│   │   ├── bars.rs
│   │   └── replay.rs
│   ├── strategy/
│   │   └── sma_cross.rs
│   ├── execution/
│   │   ├── orders.rs
│   │   ├── fills.rs
│   │   ├── portfolio.rs
│   │   └── state_machine.rs
│   └── experiments/
│       ├── faults.rs
│       ├── metrics.rs
│       └── validation.rs
├── tests/
│   ├── election_tests.rs
│   ├── replication_tests.rs
│   ├── recovery_tests.rs
│   └── determinism_tests.rs
├── config/
│   └── cluster.yaml
├── data/
│   └── spy_1min.parquet
└── results/
```

## 7. Build it in this order

Milestone 1 — Deterministic trading engine

Implement bar replay, the strategy, simulated orders, fills, and portfolio state. Verify that two independent runs produce identical state hashes.

Milestone 2 — Raft leader election

Launch three processes. Implement timeouts, terms, votes, and heartbeats. Verify exactly one leader per term.

Milestone 3 — Log replication

Implement `AppendEntries` , log matching, conflict repair, majority commitment, and ordered application. Start with simple integer commands before integrating trading.

Milestone 4 — Durable storage and recovery

Persist Raft state and logs, crash processes, restart them, and verify committed entries survive. Ideally build persistence into the earlier milestones rather than retrofitting it.

Milestone 5 — Alpaca integration

Replace test commands with historical SPY bars and strategy-generated orders. Run a complete replay through Raft.

Milestone 6 — Fault experiments

Crash the leader at different points, test client retries, restart nodes, and compare committed log prefixes and portfolio state hashes.

## 8. Essential correctness tests

Do not rely only on a successful demonstration. Raft can appear to work under normal conditions while violating safety during specific failures.

| Test                            | Expected result                          |
| ------------------------------- | ---------------------------------------- |
| Three nodes start               | One leader eventually elected            |
| Leader crashes                  | Remaining two elect a leader             |
| One follower crashes            | Writes can still commit                  |
| Two nodes crash                 | No new commits                           |
| Network partition 1 vs 2        | Majority side can progress               |
| Conflicting uncommitted entries | New leader repairs conflicts             |
| Stale candidate requests votes  | Rejected when log is not up to date      |
| Old leader rejoins              | Steps down when it learns of higher term |
| Client retries committed order  | No duplicate execution                   |
| Follower restarts               | Catches up and produces matching state   |

You should also test delayed, duplicated, and reordered RPC messages. Raft's correctness cannot depend on messages arriving promptly or in order.

The key validation condition is:

\\[ H(S_A(k))=H(S_B(k))=H(S_C(k)) \\]

for replicas that have applied the same committed prefix through index \\(k\\).

A temporary difference in `lastApplied` is normal; comparing nodes at different applied indices would produce a misleading failure.

## 9. What I would cut from the original project

Because you're implementing Raft from scratch, I would postpone cross-sectional strategies, multiple symbols, tick-by-tick quotes, historical trades, partial fills, and a realistic limit-order book.

Those are valuable extensions, but none is necessary to demonstrate consensus correctness.

Instead, use SPY OHLCV as the primary workload, with a configurable replay speed and the ability to generate synthetic order bursts. This gives you enough events to test elections, replication, leader failure, and recovery without requiring a massive historical dataset.

Once Raft works, adding quotes is a relatively contained change to the market-data and execution layers.

## 10. Final project definition

Raft-Replicated Order Matching and Execution Sandbox

Implement a three-node Raft consensus protocol from scratch in Rust, using historical SPY OHLCV data from Alpaca to drive a deterministic trading strategy. The system replicates market events and simulated orders through a persistent Raft log. Every node independently applies committed commands to identical execution and portfolio state machines. Controlled leader crashes, network partitions, and recovery experiments test whether committed orders, fills, and portfolio balances remain consistent.

The deliverables would be a working three-process cluster, a historical replay engine, a simulated execution engine, automated fault-injection tests, and a performance/correctness report.

One scope caveat: With OHLCV-only execution, this is more accurately a replicated order-execution simulator than a full order-matching engine. If the assignment specifically requires matching opposing limit orders using price-time priority, you would need to add a small synthetic limit-order book. Otherwise, OHLCV-based simulated fills are sufficient for studying Raft state-machine replication.

The foundational reference should be the extended Raft paper, In Search of an Understandable Consensus Algorithm, especially Figure 2 and Sections 5.2–5.4. Those define the election, replication, and safety rules your implementation needs to follow.
