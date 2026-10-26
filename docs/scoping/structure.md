# Initial Repository Structure — Raft-Replicated Trading Sandbox

For the MVP, I recommend a single Rust crate with modular separation, rather than a multi-crate Cargo workspace. This keeps compilation, testing, and integration simple while maintaining clear boundaries between Raft, market data, strategy, execution, and experiments.

## 1. Proposed repository structure

```
raft-trading-sandbox/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .gitignore
├── README.md
│
├── config/
│   ├── cluster.toml             # Three-node Raft configuration
│   ├── market.toml              # SPY data/replay configuration
│   └── strategy.toml            # Strategy parameters
│
├── src/
│   ├── main.rs                  # CLI entry point
│   ├── lib.rs                   # Public module declarations
│   │
│   ├── raft/                    # Custom Raft implementation
│   │   ├── mod.rs
│   │   ├── node.rs              # Node state and transitions
│   │   ├── election.rs          # Leader election and voting
│   │   ├── replication.rs       # Log replication and commit
│   │   ├── log.rs               # Log entries and consistency
│   │   ├── rpc.rs               # Raft RPC message definitions
│   │   ├── transport.rs         # TCP communication
│   │   ├── storage.rs           # Persistent Raft state
│   │   └── timer.rs             # Election and heartbeat timers
│   │
│   ├── market/                  # Historical market data
│   │   ├── mod.rs
│   │   ├── alpaca.rs            # Alpaca historical bars API
│   │   ├── bar.rs               # OHLCV data structures
│   │   └── replay.rs            # Deterministic event replay
│   │
│   ├── strategy/                # Simulated trading strategies
│   │   ├── mod.rs
│   │   └── sma_crossover.rs     # 5/20 SMA crossover
│   │
│   ├── execution/               # Replicated state machine
│   │   ├── mod.rs
│   │   ├── command.rs           # Replicated command types
│   │   ├── order.rs             # Market orders
│   │   ├── fill.rs              # Deterministic fill logic
│   │   ├── portfolio.rs         # Cash and SPY positions
│   │   └── state_machine.rs     # Apply committed commands
│   │
│   ├── experiments/             # Fault injection and metrics
│   │   ├── mod.rs
│   │   ├── faults.rs            # Leader crash/restart
│   │   ├── metrics.rs           # Latency and throughput
│   │   └── validation.rs        # State-hash comparison
│   │
│   └── common/
│       ├── mod.rs
│       ├── config.rs            # Configuration loading
│       └── error.rs             # Shared error types
│
├── tests/
│   ├── raft_election.rs
│   ├── raft_replication.rs
│   ├── raft_recovery.rs
│   ├── execution_determinism.rs
│   └── end_to_end.rs
│
├── scripts/
│   ├── start_cluster.sh         # Launch three Raft nodes
│   ├── stop_cluster.sh          # Stop local cluster
│   ├── fetch_spy.sh             # Fetch historical SPY data
│   └── run_experiment.sh        # Replay + failure injection
│
├── data/
│   ├── raw/                     # Alpaca API responses
│   ├── processed/               # Normalized SPY OHLCV
│   └── traces/                  # Immutable replay datasets
│
├── runtime/                     # Gitignored node state
│   ├── node-1/
│   ├── node-2/
│   └── node-3/
│
├── results/                     # Experiment outputs
│   ├── logs/
│   ├── metrics/
│   └── validation/
│
└── docs/
    ├── architecture.md
    ├── raft-design.md
    ├── execution-model.md
    ├── experiments.md
    └── limitations.md
```

## 2. Module responsibilities

| Module         | Main responsibility                        | Dependencies            |
| -------------- | ------------------------------------------ | ----------------------- |
| `raft/` | Consensus, leader election, replicated log | `execution::Command` |
| `market/` | Historical SPY OHLCV ingestion and replay  | Alpaca HTTP API         |
| `strategy/` | Generate trading decisions from bars       | `market/` |
| `execution/` | Apply committed commands deterministically | No Alpaca or networking |
| `experiments/` | Fault injection, measurement, validation   | Raft and execution      |
| `common/` | Configuration and shared errors            | Minimal                 |

The most important design constraint: `execution/` must never call Alpaca, access wall-clock time, generate random numbers, or depend on which Raft node is applying a command.

## 3. Core Rust interfaces

These are the interfaces I would establish before implementing the actual modules.

### `src/execution/command.rs`

```
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}
```

### `src/raft/rpc.rs`

```
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVoteRequest {
    pub term: u64,
    pub candidate_id: u64,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVoteResponse {
    pub term: u64,
    pub vote_granted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesRequest {
    pub term: u64,
    pub leader_id: u64,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesResponse {
    pub term: u64,
    pub success: bool,
}
```

`LogEntry` should be defined in `raft/log.rs` , and the RPC module should import it. Avoid maintaining duplicate log-entry definitions.

### `src/raft/node.rs`

```
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

pub struct RaftNode {
    pub id: u64,
    pub role: Role,

    // Persistent state
    pub current_term: u64,
    pub voted_for: Option<u64>,
    pub log: Vec<LogEntry>,

    // Volatile state
    pub commit_index: u64,
    pub last_applied: u64,

    // Leader-only state
    pub next_index: Vec<u64>,
    pub match_index: Vec<u64>,
}
```

Use a sentinel entry at log index 0 or explicitly define an empty-log convention. Whichever you choose, document it consistently across elections, replication, and recovery.

For production-quality structure, make Raft's mutable fields private and expose operations through methods that enforce state transitions and persistence ordering.

## 4. Recommended dependencies

Start with a small dependency set.

```
[package]
name = "raft-trading-sandbox"
version = "0.1.0"
edition = "2024"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "2"
sha2 = "0.10"
rand = "0.9"

[dev-dependencies]
tempfile = "3"
proptest = "1"
```

Version constraints are starting points, not a verified lockfile. Generate and commit `Cargo.lock` after resolving compatible versions.

For the initial MVP, I would store normalized OHLCV data in CSV or JSONL rather than immediately introducing Parquet. Parquet can be added once the replay pipeline is correct.

## 5. CLI design

One binary can support several commands:

```
# Launch a Raft node
cargo run -- node --id 1 --config config/cluster.toml

# Fetch historical SPY bars
cargo run -- fetch --symbol SPY --date 2026-10-22

# Replay the historical trace
cargo run -- replay --trace data/traces/spy.jsonl

# Validate replicated state
cargo run -- validate --config config/cluster.toml
```

These are proposed interfaces, not commands that will work until the CLI is implemented.

The `replay` command should submit to the active Raft leader, retry safely when leadership changes, and wait for each command's commit acknowledgement before advancing when strict sequencing is required.

## 6. Build order

Implement the repository in the following order:

1. Scaffold: Cargo project, module declarations, configuration, logging, and CLI.
2. Raft core: RPC definitions, node state, leader election, heartbeats.
3. Raft replication: Persistent logs, `AppendEntries`, majority commitment, and conflict repair.
4. Recovery: Restart nodes, reconstruct state, and synchronize logs.
5. Execution: Deterministic market orders, next-bar fills, and portfolio accounting.
6. Market replay: Alpaca ingestion, immutable SPY traces, and strategy-generated orders.
7. Experiments: Crash injection, state comparison, latency metrics, and end-to-end tests.

Build and test Raft initially using simple commands such as `SetValue` or `IncrementCounter` . Only integrate trading commands after the consensus tests pass.

## 7. What not to create yet

Avoid adding `frontend/` , `dashboard/` , `docker/` , `kubernetes/` , `orderbook/` , `options/` , or `multi_asset/` directories at this stage.

Also avoid prematurely splitting the project into multiple Cargo crates. If the codebase grows substantially, `raft/` , `market/` , and `execution/` can later become independent crates without changing their conceptual boundaries.

Bottom line: This repository gives you a clear separation between your original Raft implementation, the Alpaca replay pipeline, and the deterministic execution state machine. The first implementation target should be three nodes successfully electing a leader and replicating a simple integer command—not downloading market data or implementing a trading strategy.
