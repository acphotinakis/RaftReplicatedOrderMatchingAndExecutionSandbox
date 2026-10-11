# Technology and Learning Scope

## Project goal

Build a three-node C++23 sandbox that tests whether Raft log agreement, combined
with a deterministic application state machine and idempotent client semantics,
preserves identical committed orders, fills, and portfolio state through leader
failure and recovery.

The project should optimize for an understandable implementation, reproducible
failure experiments, defensible evidence, and measurable systems performance. It
is not a production exchange or a general-purpose consensus platform.

## Chosen stack

| Area | Choice | Purpose |
|---|---|---|
| Language | C++23 with Clang and GCC | Modern value types and explicit control over ownership, memory layout, allocation, and concurrency. |
| Build | CMake presets + Ninja + Conan 2 or vcpkg | Reproducible debug, sanitizer, release, benchmark, and cross-compiler builds. |
| Event runtime | Boost.Asio | Production TCP, timers, signals, cancellation, and event-loop execution outside the pure cores. |
| Wire schema | Protocol Buffers over framed TCP | Typed, versioned peer and client messages with explicit framing. |
| Byte handling | `std::span<std::byte>`, `std::vector<std::byte>`, and Asio buffers | Explicit bounded views, storage ownership, and scatter/gather operations. |
| Persistence | Project-owned append-only WAL | Makes durability, corruption detection, truncation, and recovery explicit. |
| Traces | CSV or JSON Lines | Human-readable bounded input fixtures and reproducible replay. |
| Unit testing | GoogleTest/GoogleMock | Protocol, matching, storage, and component-level verification. |
| Property testing | RapidCheck | Generated and minimized order, protocol, and failure histories. |
| Fuzzing | Clang libFuzzer | Protocol decoding, WAL recovery, and malformed-input testing. |
| Runtime analysis | ASan, UBSan, and TSan | Detect memory errors, undefined behavior, and data races in separate CI jobs. |
| Static analysis | clang-tidy | Enforce ownership, correctness, and maintainability policies. |
| Formal model | TLA+/PlusCal + TLC | Check core Raft and client-deduplication safety invariants. |
| Benchmarking | Google Benchmark + HdrHistogram | Microbenchmark regression detection and cluster tail-latency distributions. |
| Profiling | Linux `perf` + flamegraphs | CPU, cache, branch, allocation, syscall, and scheduler evidence. |
| Observability | spdlog/`fmt` + prometheus-cpp | Explain elections, replication, application, divergence, and recovery. |
| Automation | GitHub Actions | Format, static analysis, Clang/GCC builds, sanitizers, tests, fuzz smoke, model checks, and benchmark builds. |

Choose exactly one dependency manager: Conan 2 or vcpkg. Use Python only for
workload generation, statistical analysis, and plots; it must not participate in
consensus, matching, or replicated portfolio state.

## Required concepts

### Modern C++ systems programming

- RAII and deterministic destruction.
- Value semantics, move semantics, references, and smart pointers.
- Object lifetime, ownership, iterator invalidation, and buffer lifetime.
- `enum class`, `std::variant`, `std::visit`, strong wrapper types, concepts, and
  `std::expected`.
- A documented exception policy; core transition functions should expose
  expected failures explicitly.
- Threads, mutexes, channels/queues, atomics, memory ordering, and false sharing.
- Boost.Asio event loops, timers, cancellation, handlers, and asynchronous buffer
  lifetimes.
- Memory representation, alignment, cache locality, allocation, copying, strict
  aliasing, and undefined behavior.
- CMake targets and presets, compiler warnings, clang-format, clang-tidy,
  sanitizers, tests, fuzzing, and benchmarks.

Prefer value types, RAII, standard containers, and explicit ownership throughout
the protocol and matching cores. Raw pointers, custom allocators, lock-free
structures, and manual memory management require measured justification,
documented invariants, and isolated tests.

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
- RAII ownership of file descriptors and mapped regions.
- Canonical byte encoding rather than raw persistence of ABI-dependent structs.

## Architectural boundaries

```text
trace -> strategy/client -> runtime adapter -> pure Raft core
                                             -> persistence/network effects
                                             -> committed entries
                                                -> pure matcher/portfolio

production: Boost.Asio + Protocol Buffers + real files/processes
simulation: virtual clock + network + storage + process lifecycle

both execute the same protocol and application cores
```

Suggested CMake target layout:

- `domain`: commands, outcomes, IDs, prices, quantities, and validation.
- `matching`: deterministic order book, fills, portfolio, and invariants.
- `raft`: protocol state, input events, transitions, and output effects.
- `storage`: hard state, WAL, checksums, snapshots, and recovery.
- `transport`: Boost.Asio/Protocol Buffers peer and client adapters.
- `node`: Boost.Asio production shell and lifecycle management.
- `client`: leader discovery, retry, deduplication keys, and trace replay.
- `simulator`: virtual time, network, storage, failures, and seed replay.
- `benchmarks`: matcher microbenchmarks and open-loop cluster workloads.

One logical event loop should mutate each node's Raft state. Asynchronous
handlers communicate using owned typed events instead of sharing protocol
internals through references protected by many locks.

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

Model domain primitives as strong wrapper structs rather than interchangeable
integer aliases. Schema versioning is required. Rolling upgrades are not.

## Testing strategy

### Unit and property tests

- Table-driven GoogleTest protocol transition tests.
- Golden order-book and portfolio traces.
- Generated valid and invalid order streams using RapidCheck.
- Invariants after every generated command, not just final-state checks.
- libFuzzer targets for protocol decoding and WAL recovery.
- Separate ASan, UBSan, and TSan test configurations.
- clang-tidy in CI for static-analysis findings.

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

The model does not prove the C++ implementation correct. It clarifies intended
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
- allocations and bytes copied per committed command;
- cycles, instructions, branch misses, and LLC misses;
- WAL synchronization latency;
- replication lag;
- election/failover interruption;
- restart replay and follower catch-up time.

Use an open-loop load generator, controlled CPU configuration, warm-up, repeated
runs, confidence intervals, raw artifacts, and complete environment metadata.
Record compiler and standard-library versions, optimization flags, LTO state,
and target CPU. Never present sanitizer-build timings as release performance.

Only optimize a measured bottleneck. Candidate experiments include `std::pmr`
order arenas, bounded SPSC queues, alternative encodings, core pinning,
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

1. C++23 ownership, RAII, value/move semantics, variants, error handling,
   concurrency, atomics, the memory model, and Asio lifetime rules.
2. Raft paper, invariants, and a small TLA+ model.
3. Pure matching/portfolio state machine and property tests.
4. Pure in-memory Raft transition core.
5. Seeded deterministic simulator and fault histories.
6. Committed application, state digests, and replicated deduplication.
7. Checksummed WAL and restart recovery.
8. Boost.Asio/Protocol Buffers production shell and three real processes.
9. Full failure matrix, sanitizers, fuzzing, static analysis, and model checking.
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
- **C++ quality:** release, Clang, GCC, static-analysis, sanitizer, fuzzing, and
  deterministic-simulation checks are clean for their stated scope.
- **Measured cost:** the report isolates matching, serialization, persistence,
  replication, and failover costs without overstating production applicability.

## Primary references

- Diego Ongaro and John Ousterhout, [In Search of an Understandable Consensus
  Algorithm](https://raft.github.io/raft.pdf).
- Diego Ongaro, [Consensus: Bridging Theory and Practice](https://github.com/ongardie/dissertation/blob/master/stanford.pdf).
- [C++ Core Guidelines](https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines).
- [cppreference](https://en.cppreference.com/).
- [Boost.Asio overview](https://www.boost.org/latest/doc/html/boost_asio/overview/basics.html).
- [Protocol Buffers C++ tutorial](https://protobuf.dev/getting-started/cpptutorial/).
- [eBay NuRaft](https://github.com/eBay/NuRaft) as a reference design.
- [GoogleTest](https://github.com/google/googletest),
  [RapidCheck](https://github.com/emil-e/rapidcheck), and
  [Google Benchmark](https://github.com/google/benchmark).
- [LLVM sanitizers](https://clang.llvm.org/docs/index.html),
  [LLVM libFuzzer](https://llvm.org/docs/LibFuzzer.html), and
  [clang-tidy](https://clang.llvm.org/extra/clang-tidy/).
- [TLA+ examples](https://github.com/tlaplus/Examples).
- [HdrHistogram](https://github.com/HdrHistogram/HdrHistogram).

When references or generative AI materially influence submitted work,
acknowledge them according to the course academic-honesty policy.
