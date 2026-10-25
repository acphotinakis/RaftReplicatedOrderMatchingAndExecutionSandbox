# Quant-Focused Technology Stack Research

*Research date: October 6, 2026 | Decision: Rust | Confidence: high*

## Executive decision

Use **Rust 2024 on Linux** for the complete assessed system.

The goal is not to imitate one firm's private production stack. The goal is to
produce a polished artifact demonstrating abilities that transfer across
quantitative trading firms:

* deterministic execution; 
* distributed consensus and failure recovery; 
* memory-safe, race-resistant systems programming; 
* explicit durability and retry semantics; 
* adversarial and reproducible testing; 
* mechanical sympathy backed by measurements; 
* clear communication of safety, liveness, and latency tradeoffs.

Rust fits this project because its hardest problems are protocol state, 
concurrency, persistence, and deterministic recovery—not an exchange gateway's
final nanoseconds. Rust provides low-level control without a garbage collector
while preventing broad classes of memory and data-race bugs. That increases the
chance of completing the consensus experiment, simulator, recovery logic, and
performance study within one semester.

The intended result is:

> A memory-safe, crash-consistent Raft-replicated matching engine with
> deterministic failure simulation, machine-checked invariants, reproducible
> fault histories, and a rigorous analysis of consensus and failover costs.

## Industry evidence

### Jump Trading

Jump's public technology page explicitly lists high-performance frameworks in
Rust, C++, Python, and Go. It also highlights distributed systems, simulation, 
custom hardware, proprietary networking, and large research infrastructure.
This is strong first-party evidence that Rust is relevant to modern trading
infrastructure, without implying that it replaces C++ everywhere.

**Project signal:** safe high-performance systems work, simulation, distributed
correctness, networking, and measurement.

Sources:

* [Jump Trading technology](https://www.jumptrading.com/technology)
* [Jump C++ trading role](https://www.jumptrading.com/hr/job?gh_jid=8242766)

### Jane Street

Jane Street is primarily an OCaml organization, so copying a C++ stack would not
match it either. Its engineering material emphasizes strong types, determinism, 
replay, reliable state machines, tail latency, allocation behavior, cache
effects, and careful measurement.

The “How to Build an Exchange” talk is especially relevant: it discusses
deterministic response, replay-based recovery, replicated state, low-allocation
design, and consensus latency. Jane Street's programming-language work also
discusses Rust-style ownership and data-race freedom as valuable high-performance
language capabilities.

**Project signal:** use Rust's type system thoughtfully, make nondeterminism
explicit, and measure rather than merely claim performance.

Sources:

* [Jane Street technology](https://www.janestreet.com/technology/)
* [How to Build an Exchange](https://www.janestreet.com/tech-talks/building-an-exchange/)
* [System Jitter and Where to Find It](https://www.janestreet.com/tech-talks/system-jitter-and-where-to-find-it/)
* [Safe at Any Speed](https://www.janestreet.com/tech-talks/safe-at-any-speed/)
* [Making OCaml Safe for Performance Engineering](https://www.janestreet.com/tech-talks/making-ocaml-safe-for-performance-engineering/)

### Optiver

Optiver's public low-latency roles strongly feature C++, but its broader systems
roles also recognize Rust. More important here, Optiver repeatedly signals
deterministic real-time behavior, distributed-systems fundamentals, reliability, 
market-data processing, execution logic, simulation, Linux, networking, and
observability.

**Project signal:** connect Rust to exchange-style domain modeling and produce
observable, deterministic, performance-conscious behavior.

Sources:

* [Algorithmic Trading C++ Engineer](https://www.optiver.com/join-us/jobs/technology/sydney/algorithmic-trading-c-plus-plus-engineer-equities/)
* [Senior Production Software Engineer](https://www.optiver.com/join-us/jobs/technology/chicago/senior-production-software-engineer/)
* [Software Engineer, Equities and ETFs](https://www.optiver.com/join-us/jobs/technology/new-york/software-engineer-equities-etfs/)

### Citadel and Citadel Securities

Citadel's public execution roles strongly favor modern C++, but the underlying
requirements are broader: concurrency, distributed systems, reliability, 
fail-safe behavior, testing, real-time data, simulation, and measurable
performance. A serious Rust implementation can demonstrate those capabilities
even when the target team uses C++.

**Project signal:** show command of systems fundamentals and performance rather
than relying on Rust as a substitute for understanding memory layout, CPU caches, 
networking, or durability.

Sources:

* [Citadel engineering](https://www.citadel.com/careers/engineering/)
* [GQS C++ Quantitative Research Engineer](https://www.citadel.com/careers/details/global-quantitative-strategies-c-quantitative-research-engineer/)
* [Citadel Securities C++ Software Engineer](https://www.citadelsecurities.com/careers/details/c-software-engineer-2/)
* [Citadel Securities SDET](https://www.citadelsecurities.com/careers/details/software-engineer-in-test-sdet/)

## Why Rust over C++ for this project

| Project need | Rust advantage |
|---|---|
| Dynamic protocol state | Enums and exhaustive `match` expressions make transitions and missing cases explicit. |
| Concurrent RPC processing | Ownership plus `Send` / `Sync` eliminates broad classes of unsafe sharing and data races. |
| Durable buffers and messages | Lifetimes prevent many use-after-free and asynchronous callback errors. |
| Error handling | `Result` makes transport, decoding, storage, and recovery failures part of the API. |
| Deterministic domain model | Newtypes and algebraic data types prevent accidental mixing of prices, quantities, IDs, and commands. |
| Testing | `proptest` , Loom, fuzzing, and deterministic simulation provide a strong correctness story. |
| Build reproducibility | Cargo workspaces, locked dependencies, formatting, linting, testing, and documentation are integrated. |
| Semester risk | Fewer memory-corruption and undefined-behavior failures leave more time for the actual experiment. |

C++ remains more common in public low-latency execution roles. The project
should acknowledge that rather than claiming Rust has displaced it. This choice
is based on project fit and differentiation: Rust makes it more feasible to
deliver a deep distributed-systems artifact instead of spending the semester
debugging memory lifetime and build-system problems.

Do not split the core between Rust and C++. Python may be used off the
correctness path for workload generation, statistical analysis, and plotting.

## Recommended Rust stack

### Language and build

* **Stable Rust, edition 2024**.
* A Cargo workspace with small crates and a committed `Cargo.lock`.
* `rustfmt`, Clippy with warnings denied in CI, and `cargo doc`.
* Linux as the benchmark and deployment target; macOS may be used for ordinary
  development.
* Release profiles recorded with every benchmark. Treat LTO, target-specific CPU
  flags, and alternative allocators as separately measured configurations.

### Runtime and networking

* **Tokio** for the production runtime, TCP, timers, signals, and tasks.
* **Tonic + Prost/Protocol Buffers** for the initial typed RPC schema.
* `bytes` for bounded byte buffers and framing.
* `tower` middleware only for explicit timeout, limit, or observability policy.
* `tracing` and `tracing-subscriber` for structured event timelines.

Keep Tokio outside the consensus and matching cores. Those cores should not be
`async` , read clocks, open sockets, or perform disk I/O. The runtime translates
external events into explicit inputs and executes returned effects.

### Consensus

* Implement the restricted Raft core directly from Ongaro and Ousterhout:
  fixed three-node membership, election, heartbeats, replication, conflict
  repair, majority commit, ordered application, and crash/restart recovery.
* Express it as a pure event-driven state machine:

```text
(raft_state, input_event)
    -> new_state
    + messages_to_send
    + records_to_persist
    + committed_entries_to_apply
```

* Represent the dynamic role with an enum rather than an elaborate generic
  type-state hierarchy:

```rust
enum Role {
    Follower,
    Candidate(CandidateState),
    Leader(LeaderState),
}
```

* Use [`raft-rs`](https://github.com/tikv/raft-rs) and
  [OpenRaft](https://docs.rs/openraft/latest/openraft/) as references or optional
  comparisons, not substitutes for the assessed Raft core.
* Exclude membership changes, leases, pre-vote, leadership transfer, and
  optimized reads from the MVP.

### Deterministic matching and portfolio state

* One pure transition interface:

```text
(application_state, committed_command) -> new_state + deterministic_events
```

* Newtypes for `PriceTicks`,  `QuantityLots`,  `OrderId`,  `ClientId`, and sequence
  numbers.
* Integer ticks and lots; no floating point in replicated state transitions.
* Limit orders with explicit price-time priority for the MVP.
* Ordering based on committed log index plus an intra-entry ordinal.
* Ordered collections or explicit sorting wherever iteration affects output.
* Checked or widened arithmetic with a documented overflow policy.
* Replicated deduplication keyed by `(client_id, request_sequence)`.
* No wall clock, randomness, locale, node identity, pointer address, or async
  work inside `apply()` .

### Persistence

* A project-owned append-only WAL using ordinary file I/O first.
* Record layout: magic, schema version, length, term, index, payload, and CRC32C.
* Explicit short-write, interruption, partial-tail, checksum, truncation, and
  restart handling.
* Defined synchronization and acknowledgment contract.
* Durable `current_term`,  `voted_for`, and log entries before protocol responses
  whose correctness depends on them.
* Snapshots only after log recovery works. A snapshot includes its last included
  term/index, application state, deduplication state, and canonical digest.
* Named failpoints before and after append, sync, truncate, snapshot rename, 
  apply, and client response.

Do not begin with memory-mapped persistence. `memmap2` and `io_uring` are valid
post-MVP experiments, but neither automatically provides correct durability.

### Testing and verification

* Built-in Rust unit and integration tests.
* [`proptest`](https://github.com/proptest-rs/proptest) for generated and shrunk
  commands, invariants, and failure histories.
* `cargo-fuzz`/libFuzzer for decoders, WAL recovery, and malformed frames.
* [Loom](https://docs.rs/loom/latest/loom/) for small shared-memory adapters, 
  with its model limitations documented.
* Miri where supported for unsafe-code and undefined-behavior checks.
* A seeded deterministic simulator with virtual time, stable event ordering, 
  simulated network/disk, and crash/restart controls.
* A small **TLA+/PlusCal** model checked by TLC for election safety, log matching, 
  leader completeness/state-machine safety, and client deduplication.

Every failure must print a replayable seed and fault history. Prefer no `unsafe`

in the protocol and domain cores. If an optimized adapter eventually uses
`unsafe` , isolate it, document its invariants, and test it independently.

### Performance engineering

* **Criterion.rs** for matcher, codec, hashing, and WAL microbenchmarks.
* **HdrHistogram** for end-to-end p50/p95/p99/p99.9/max distributions.
* Linux `perf`, flamegraphs, and hardware counters.
* Allocation instrumentation for allocations per command.
* Documented CPU affinity and governor settings for controlled benchmarks.
* An open-loop load generator to avoid hiding overload through coordinated
  omission.

Avoid “zero allocation, ” “lock-free, ” “zero copy, ” and “nanosecond latency” as
design slogans. Establish a correct baseline, profile it, optimize one measured
bottleneck, and publish the before/after result and its tradeoff.

### Observability and artifacts

* Structured `tracing` events with node, term, role, peer, log index, commit
  index, applied index, and stable request ID.
* Prometheus-format counters and histograms; OpenTelemetry is optional.
* A terminal dashboard showing nodes, leader changes, replication lag, and state
  digests.
* JSON/CSV experiment artifacts and scripts or notebooks for plots.
* GitHub Actions checks for format, Clippy, tests, deterministic scenarios, 
  fuzz smoke tests, model checking, documentation, and benchmark compilation.

## Architecture

```text
bounded trace -> strategy/client -> leader gateway
                                  -> Tokio/Tonic adapter
                                  -> pure Raft transition core
                                     -> WAL and peer effects
                                     -> committed command stream
                                        -> pure matching/portfolio state

seeded simulator -> virtual clock/network/disk/process lifecycle
production shell -> Tokio clock/TCP/files/process lifecycle

both environments execute the same protocol and application cores
```

Suggested workspace:

```text
crates/
  domain/       strongly typed commands, results, IDs, and quantities
  matching/     pure order book, fills, portfolio, and invariants
  raft/         pure Raft state, inputs, transitions, and effects
  storage/      WAL, hard state, checksums, snapshots, and recovery
  transport/    Tonic/Prost service and peer adapters
  node/         production runtime wiring
  client/       retry, leader discovery, idempotency, and trace replay
  simulator/    virtual time, network, storage, crashes, and seeds
  benchmarks/   microbenchmarks and open-loop cluster driver
spec/
  tla/          model, configuration, invariants, and checked output
experiments/    scenarios, raw results, metadata, and plots
```

Use one logical owner/event loop for each node's mutable Raft state. Channels
carry immutable inputs and effects rather than expose consensus internals.

## Correctness experiment

At each applied index, compute canonical component digests for the committed
command prefix, orders, fills, portfolios, and client deduplication results.
Canonical encoding must sort keys and encode logical fields explicitly; never
hash raw memory, padding, addresses, or incidental map order.

Required scenarios:

01. No-fault golden replay.
02. Leader crash before local append.
03. Crash after local append but before majority replication.
04. Crash after majority commit but before client response.
05. Old leader isolated, new leader commits traffic, partition healed.
06. Delayed, reordered, duplicated, and dropped Raft messages.
07. Follower crash during append, restart, and catch-up.
08. Divergent uncommitted suffix repair.
09. Interrupted snapshot installation and retry.
10. Repeated elections followed by network stability and eventual progress.

At every common applied index, replicas must have the same committed prefix and
state digest. A retry after a lost response must return the original replicated
result without applying the logical order twice.

Use precise terminology: linearizable committed writes plus replicated
idempotency provide effectively-once logical execution under stated
crash-recovery assumptions. They do not provide exactly-once delivery or
Byzantine tolerance.

## Performance study

Measure four baselines:

01. Pure matching without logging or replication.
02. Single-node WAL-backed matching.
03. Three-node Raft with memory-only storage.
04. Three-node Raft with durable majority acknowledgment.

Cover add, cancel, partial/full fill, shallow/deep books, bursts, sustained load, 
batch sizes, WAL policies, network RTT, and leader failure at each replication
boundary.

Report throughput and saturation; p50/p95/p99/p99.9/max latency; matcher time and
allocations; cycles, instructions, branch and LLC misses; serialization cost; 
WAL sync distribution; failover interruption; and recovery/catch-up time.

Commit raw results, code revision, workload seed, hardware, kernel, Rust version, 
compiler flags, warm-up policy, run count, and confidence intervals. Treat
outliers as observations to explain rather than silently discard.

## High-value extensions

Only after the correctness matrix passes:

01. Preallocated slab or arena for orders, measured against the baseline.
02. Bounded SPSC channel if profiling shows contention or scheduler interference.
03. FlatBuffers or `rkyv`, with validation and compatibility analysis.
04. Memory-mapped or `io_uring` WAL with explicit crash tests.
05. Core pinning and cache-line layout experiments with hardware-counter evidence.
06. An OpenRaft- or `raft-rs`-backed comparison implementation.

Do not add DPDK, RDMA, FPGA work, Kubernetes, Kafka, or a polished web dashboard
until the core safety and measurement objectives are complete.

## Delivery sequence

01. Rust ownership, enums, errors, traits, concurrency, and async boundaries.
02. Pure domain and matching state machine with golden/property tests.
03. Pure in-memory Raft core with deterministic unit scenarios.
04. Seeded simulator using the same Raft core.
05. Committed application, canonical digests, and client deduplication.
06. Checksummed WAL, restart replay, and storage failpoints.
07. Tokio/Tonic production adapter and real three-process cluster.
08. Fuzzing, Loom checks, TLA+ model, and full failure matrix.
09. Performance baselines, raw artifacts, charts, and demo.
10. One measured extension if schedule permits.

## Resume-worthy outcome

> Built a Rust three-node Raft-replicated limit-order-book simulator with a
> crash-consistent checksummed WAL, replicated request deduplication, and pure
> deterministic consensus and matching cores. Model-checked safety invariants in
> TLA+, reproduced crash and partition histories in a seeded simulator, and
> quantified consensus and failover costs using open-loop load generation, HDR
> histograms, and Linux hardware counters.

Add measured numbers only after the experiments exist.

## Evidence limits and primary references

Public job descriptions and talks show recruiting and communication priorities, 
not complete private production stacks. The Rust decision is an engineering
judgment based on this project's distributed-systems scope and semester
constraints—not a claim that Rust dominates every target firm.

* [Raft paper and resources](https://raft.github.io/)
* [The Rust Programming Language](https://doc.rust-lang.org/book/)
* [Tokio documentation](https://tokio.rs/)
* [Tonic](https://github.com/hyperium/tonic)
* [Prost](https://github.com/tokio-rs/prost)
* [`raft-rs`](https://github.com/tikv/raft-rs)
* [OpenRaft](https://docs.rs/openraft/latest/openraft/)
* [`proptest`](https://github.com/proptest-rs/proptest)
* [Loom](https://docs.rs/loom/latest/loom/)
* [Criterion.rs](https://github.com/criterion-rs/criterion.rs)
* [HdrHistogram](https://github.com/HdrHistogram/HdrHistogram)
* [TLA+ examples](https://github.com/tlaplus/Examples)

When references or generative AI materially influence submitted work, 
acknowledge them according to the course academic-honesty policy.
