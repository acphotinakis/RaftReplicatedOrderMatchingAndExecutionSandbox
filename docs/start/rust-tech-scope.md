# Technology and Learning Scope

## Project goal

Build a three-node Rust sandbox that tests whether Raft log agreement, combined
with a deterministic application state machine and idempotent client semantics,
preserves identical committed orders, fills, and portfolio state through leader
failure and recovery.

The project should optimize for an understandable implementation, reproducible
failure experiments, and defensible evidence. It is not a production exchange
or a general-purpose consensus platform.

## Chosen stack

| Area | Choice | Purpose |
|---|---|---|
| Language | Stable Rust, edition 2024 | Memory-safe systems programming without garbage collection; strong domain and protocol types. |
| Build | Cargo workspace + locked dependencies | Reproducible builds, tests, benchmarks, documentation, and dependency management. |
| Async runtime | Tokio | Production TCP, timers, signals, and task execution outside the pure cores. |
| RPC schema | Tonic + Prost/Protocol Buffers | Typed, versioned peer and client messages. |
| Byte handling | `bytes` | Bounded framing and buffer management. |
| Persistence | Project-owned append-only WAL | Makes durability, corruption detection, truncation, and recovery explicit. |
| Traces | CSV or JSON Lines | Human-readable bounded input fixtures and reproducible replay. |
| Property testing | `proptest` | Generated and minimized order, protocol, and failure histories. |
| Concurrency testing | Loom | Bounded exploration of small shared-memory components. |
| Fuzzing | `cargo-fuzz`/libFuzzer | Protocol decoding, WAL recovery, and malformed-input testing. |
| Formal model | TLA+/PlusCal + TLC | Check core Raft and client-deduplication safety invariants. |
| Benchmarking | Criterion.rs + HdrHistogram | Microbenchmark regression detection and cluster tail-latency distributions. |
| Profiling | Linux `perf` + flamegraphs | CPU, cache, branch, allocation, and scheduler evidence. |
| Observability | `tracing` + Prometheus metrics | Explain elections, replication, application, divergence, and recovery. |
| Automation | GitHub Actions | Format, lint, test, model, fuzz-smoke, and benchmark-build checks. |

Use Python only for workload generation, statistical analysis, and plots. It
must not participate in consensus, matching, or replicated portfolio state.

## Required concepts

### Rust systems programming

- Ownership, borrowing, lifetimes, moves, and interior mutability.
- `Send`, `Sync`, channels, atomics, mutexes, and task cancellation.
- Enums, exhaustive matching, newtypes, traits, generics, and error types.
- Async Rust and the boundary between protocol logic and runtime effects.
- Memory representation, alignment, cache locality, allocation, and copying.
- Safe versus `unsafe` Rust and how to document an unsafe contract.
- Cargo workspaces, features, profiles, Clippy, rustfmt, tests, and benchmarks.

Prefer safe Rust throughout the protocol and matching cores. Unsafe code is not
a performance credential by itself; use it only for a measured extension with a
small, documented, independently tested boundary.

### Raft and replicated state machines

- Follower, candidate, and leader roles.
- Terms, randomized election timeouts, heartbeats, and majority voting.
- `RequestVote` and the log up-to-date rule.
- `AppendEntries`, prefix matching, conflict repair, and suffix truncation.
- Majority commitment, current-term commit restriction, and ordered apply.
- Persistent versus volatile Raft state.
- Election safety, log matching, leader completeness, and state-machine safety.
- Stable client request IDs, retries, redirection, and replicated deduplication.
- Crash-stop and crash-recovery behavior.
- Safety versus liveness under minority and majority partitions.

The MVP uses exactly three nodes and fixed membership. Joint consensus, learners,
leadership transfer, read leases, pre-vote, and optimized reads are out of scope.

### Deterministic matching

- Central-limit order book and price-time priority.
- Limit orders for the MVP; cancellation follows stable submission.
- Integer price ticks and quantity lots.
- Stable identifiers supplied by the client or derived from committed data.
- Committed log index plus intra-entry ordinal as authoritative logical time.
- Ordered data structures or explicit sorting for observable iteration.
- Pure application:

```text
(previous_state, committed_command) -> new_state + deterministic_events
```

- No wall clock, random number, floating point, node identity, pointer address,
  locale, unordered iteration, or asynchronous work inside the transition.

Every state-changing input must enter the log. If market trade or quote events
affect fills, those events must be committed commands too. Replicating strategy
orders while replicas consume market data independently would leave critical
nondeterminism outside Raft.

### Portfolio accounting

- Cash, position, reserved quantity/cash, fills, and order status.
- Atomic application of fills to order and portfolio state.
- No overfills or negative remaining quantities.
- Explicit treatment of deposits, external counterparties, and conservation.
- Checked arithmetic and documented overflow behavior.
- Canonical serialization and hashing for cross-replica comparison.

Use one instrument and the smallest explicit account/counterparty model that
answers the research question. Exclude fees, margin, and short selling initially.

### Durability and recovery

- Write-ahead logging, record framing, checksums, and partial trailing records.
- Short writes, interrupted system calls, log truncation, and atomic replacement.
- The difference between written, synchronized, replicated, committed, applied,
  and acknowledged.
- Durable Raft hard state: current term and vote.
- Snapshot contents, installation, and log compaction after MVP recovery works.
- Client ambiguity when a leader commits but crashes before responding.

## Architectural boundaries

```text
trace -> strategy/client -> runtime adapter -> pure Raft core
                                             -> persistence/network effects
                                             -> committed entries
                                                -> pure matcher/portfolio

production: Tokio + Tonic + real files/processes
simulation: virtual clock + network + storage + process lifecycle

both execute the same protocol and application cores
```

Suggested Cargo workspace:

- `domain`: commands, outcomes, IDs, prices, quantities, and validation.
- `matching`: deterministic order book, fills, portfolio, and invariants.
- `raft`: protocol state, input events, transitions, and output effects.
- `storage`: hard state, WAL, checksums, snapshots, and recovery.
- `transport`: Tonic/Prost services and peer/client adapters.
- `node`: Tokio production shell and lifecycle management.
- `client`: leader discovery, retry, deduplication keys, and trace replay.
- `simulator`: virtual time, network, storage, failures, and seed replay.
- `benchmarks`: matcher microbenchmarks and open-loop cluster workloads.

One logical owner should mutate each node's Raft state. RPC tasks communicate by
typed events instead of sharing protocol internals behind many locks.

## Protocol and data model

Keep Raft messages separate from replicated application commands.

- Consensus RPCs: `RequestVote`, `RequestVoteResponse`, `AppendEntries`, and
  `AppendEntriesResponse`.
- Log entry: schema version, index, term, command type, and command payload.
- Commands: `MarketEvent`, `SubmitOrder`, and later `CancelOrder`.
- Outcomes: accepted, rejected, fill, cancel result, and portfolio delta.
- Idempotency: `client_id` plus monotonically increasing `request_sequence`.
- Inspection: node ID, role, term, leader, log tail, commit index, applied index,
  replication progress, and component state digests.

Schema versioning is required. Rolling upgrades are not.

## Testing strategy

### Unit and property tests

- Table-driven protocol transition tests.
- Golden order-book and portfolio traces.
- Generated valid and invalid order streams using `proptest`.
- Invariants after every generated command, not just final-state checks.
- Decoder and WAL-recovery fuzz targets.
- Loom tests only for small shared-memory adapters where exhaustive exploration
  is tractable.

### Deterministic distributed simulation

Build a seeded discrete-event simulator with:

- virtual monotonic time;
- deterministic event ordering;
- message delay, drop, duplicate, and reorder;
- asymmetric partitions;
- node pause, crash, and restart;
- storage rejection, partial write, or crash failpoints where modeled;
- replayable seed and event history for every failure.

The simulator is a primary deliverable, not disposable test scaffolding.

### Formal model

Create a small TLA+/PlusCal model for the fixed three-node system. Check:

- at most one leader per term;
- log matching;
- leader completeness/state-machine safety;
- monotonic commit and apply indices;
- no duplicated logical application after client retry.

The model does not prove the Rust implementation correct. It clarifies intended
invariants and provides traces against which implementation scenarios can be
compared.

### Required failure matrix

1. No-fault reference replay.
2. Leader failure before local append.
3. Leader failure after local append but before majority replication.
4. Failure after majority commit but before client response.
5. Old leader partitioned, new leader commits traffic, partition heals.
6. Stale-term messages and delayed election responses.
7. Duplicate, reordered, delayed, and dropped replication messages.
8. Divergent uncommitted suffix repair.
9. Follower crash, restart, and catch-up.
10. Interrupted snapshot installation and retry, after snapshots exist.

At every shared applied index, replicas must have identical canonical component
digests. No acknowledged command may be lost, and one logical request may not
produce more than one logical effect.

## Performance plan

Correctness precedes optimization. Establish these baselines:

1. Pure matching without logging.
2. Single-node WAL-backed matching.
3. Three-node memory-only Raft.
4. Three-node durable Raft.

Measure:

- matcher add/cancel/match time and allocations;
- committed commands per second and saturation point;
- p50, p95, p99, p99.9, and maximum latency;
- bytes copied and serialized per command;
- cycles, instructions, branch misses, and LLC misses;
- WAL synchronization latency;
- replication lag;
- election/failover interruption;
- restart replay and follower catch-up time.

Use an open-loop load generator, controlled CPU configuration, warm-up, repeated
runs, confidence intervals, raw artifacts, and complete environment metadata.

Only optimize a measured bottleneck. Candidate experiments include preallocated
order slabs, bounded SPSC channels, alternative encodings, core pinning,
cache-line layout, `io_uring`, and memory-mapped logging. Each must retain a
correct baseline and publish a before/after comparison.

## Explicit non-goals

- Byzantine fault tolerance.
- Real brokerage or exchange connectivity.
- Claims of production HFT latency.
- Dynamic membership, multi-Raft, or distributed matching shards.
- Margin, short selling, derivatives, or a complete order-type catalog.
- TLS/PKI, authentication, authorization, and geo-replication.
- Kafka, Kubernetes, DPDK, RDMA, FPGA implementation, or a web dashboard before
  correctness and experimental evidence are complete.
- “Exactly-once delivery”; the goal is effectively-once logical execution using
  replicated idempotency under stated crash-recovery assumptions.

## Learning and delivery order

1. Rust ownership, error handling, enums, traits, concurrency, and async bounds.
2. Raft paper, invariants, and a small TLA+ model.
3. Pure matching/portfolio state machine and property tests.
4. Pure in-memory Raft transition core.
5. Seeded deterministic simulator and fault histories.
6. Committed application, state digests, and replicated deduplication.
7. Checksummed WAL and restart recovery.
8. Tokio/Tonic production shell and three real processes.
9. Full failure matrix, fuzzing, Loom checks, and model checking.
10. Performance baselines, report, terminal demo, and one measured extension.

## Success criteria

The project succeeds when it demonstrates, with machine-readable artifacts:

- **Raft agreement:** replicas converge on the same committed command prefix.
- **Application determinism:** the same prefix produces identical orders, fills,
  portfolios, and deduplication results.
- **Retry correctness:** failure after commit but before response does not
  duplicate a logical order.
- **Crash recovery:** restarted replicas recover durable state and catch up.
- **Liveness under assumptions:** a stable majority eventually elects a leader
  and resumes commits.
- **Measured cost:** the report isolates matching, serialization, persistence,
  replication, and failover costs without overstating production applicability.

## Primary references

- Diego Ongaro and John Ousterhout, [In Search of an Understandable Consensus
  Algorithm](https://raft.github.io/raft.pdf).
- Diego Ongaro, [Consensus: Bridging Theory and Practice](https://github.com/ongardie/dissertation/blob/master/stanford.pdf).
- [The Rust Programming Language](https://doc.rust-lang.org/book/).
- [Tokio documentation](https://tokio.rs/).
- [Tonic](https://github.com/hyperium/tonic) and
  [Prost](https://github.com/tokio-rs/prost).
- [`raft-rs`](https://github.com/tikv/raft-rs) and
  [OpenRaft](https://docs.rs/openraft/latest/openraft/) as reference designs.
- [`proptest`](https://github.com/proptest-rs/proptest),
  [Loom](https://docs.rs/loom/latest/loom/), and
  [Criterion.rs](https://github.com/criterion-rs/criterion.rs).
- [TLA+ examples](https://github.com/tlaplus/Examples).
- [HdrHistogram](https://github.com/HdrHistogram/HdrHistogram).

When references or generative AI materially influence submitted work,
acknowledge them according to the course academic-honesty policy.