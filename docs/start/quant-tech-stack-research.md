# Quant-Focused Technology Stack Research

*Research date: October 6, 2026 | Decision: C++23 | Confidence: high*

## Executive decision

Use **C++23 on Linux** for the complete assessed system.

The goal is not to imitate one firm's private production stack. The goal is to
produce a polished artifact demonstrating abilities that transfer across
quantitative trading firms:

- deterministic execution;
- distributed consensus and failure recovery;
- performance-sensitive systems programming with explicit ownership, memory
  layout, and concurrency control;
- explicit durability and retry semantics;
- adversarial and reproducible testing;
- mechanical sympathy backed by measurements;
- clear communication of safety, liveness, and latency tradeoffs.

C++23 is the strongest fit for the project's recruiting objective. Public roles
at Optiver, Citadel, Citadel Securities, and Jump directly connect modern C++ to
market data, execution, simulation, distributed systems, and low-latency
infrastructure. C++ also makes memory layout, allocation, cache behavior,
atomics, buffer ownership, and operating-system interactions visible engineering
decisions rather than hidden implementation details.

The intended result is:

> A crash-consistent C++23 Raft-replicated matching engine with deterministic
> failure simulation, machine-checked invariants, reproducible fault histories,
> and a rigorous analysis of consensus and failover costs.

The implementation risk is higher than in a memory-safe systems language. The
project controls that risk through RAII, value semantics, single-owner event
loops, narrow interfaces, compiler warnings, static analysis, sanitizers,
fuzzing, and deterministic simulation.

## Industry evidence

### Jump Trading

Jump's public technology page lists high-performance frameworks in C++, Rust,
Python, and Go, alongside distributed systems, simulation, custom hardware,
proprietary networking, and large research infrastructure. Its public C++
trading role emphasizes execution algorithms, resource efficiency, hardware
optimization, and comprehensive testing.

**Project signal:** high-performance C++, distributed-systems architecture,
simulation, hardware awareness, and evidence-driven testing.

Sources:

- [Jump Trading technology](https://www.jumptrading.com/technology)
- [Jump C++ trading role](https://www.jumptrading.com/hr/job?gh_jid=8242766)

### Jane Street

Jane Street is primarily an OCaml organization, so the goal is not to copy its
language stack. Its engineering material instead emphasizes transferable
systems skills: deterministic state machines, replay-based recovery, reliable
replication, low-allocation data paths, cache behavior, hardware counters, tail
latency, and jitter analysis.

The “How to Build an Exchange” talk is especially relevant because it discusses
deterministic response, replay-based recovery, replicated state, low-allocation
design, and the latency cost of consensus. Its performance talks reinforce that
the compelling signal is disciplined measurement rather than a language label.

**Project signal:** make nondeterminism explicit, reason precisely about state,
and validate every performance claim with reproducible measurements.

Sources:

- [Jane Street technology](https://www.janestreet.com/technology/)
- [How to Build an Exchange](https://www.janestreet.com/tech-talks/building-an-exchange/)
- [System Jitter and Where to Find It](https://www.janestreet.com/tech-talks/system-jitter-and-where-to-find-it/)
- [Safe at Any Speed](https://www.janestreet.com/tech-talks/safe-at-any-speed/)

### Optiver

Optiver's public engineering roles directly connect modern C++ with globally
distributed, low-latency trading systems, deterministic real-time behavior,
market-data ingestion, execution logic, simulation, Linux, networking,
reliability, and observability.

**Project signal:** use modern C++ to demonstrate exchange-style domain
modeling, deterministic execution, Linux systems programming, observability, and
evidence-driven latency optimization.

Sources:

- [Algorithmic Trading C++ Engineer](https://www.optiver.com/join-us/jobs/technology/sydney/algorithmic-trading-c-plus-plus-engineer-equities/)
- [Senior C++ Software Engineer, Critical Engineering](https://www.optiver.com/join-us/jobs/technology/sydney/senior-c-plus-plus-software-engineer-critical-engineering/)
- [Software Engineer, Equities and ETFs](https://www.optiver.com/join-us/jobs/technology/new-york/software-engineer-equities-etfs/)

### Citadel and Citadel Securities

Citadel says modern C++ is central to its systematic-investing platform,
including real-time data ingestion, analytics, forecasting, and execution.
Citadel Securities roles similarly emphasize C++, multithreading, distributed
systems, latency, reliability, throughput, exchange simulation, and
order-management workflows.

**Project signal:** demonstrate production-quality C++, concurrency discipline,
fault-injection testing, measurable performance, and explicit reasoning about
memory, CPU caches, networking, and durability.

Sources:

- [Citadel engineering](https://www.citadel.com/careers/engineering/)
- [GQS C++ Quantitative Research Engineer](https://www.citadel.com/careers/details/global-quantitative-strategies-c-quantitative-research-engineer/)
- [Citadel Securities C++ Software Engineer](https://www.citadelsecurities.com/careers/details/c-software-engineer-2/)
- [Citadel Securities SDET](https://www.citadelsecurities.com/careers/details/software-engineer-in-test-sdet/)

## Why C++23 for this project

| Project need | C++23 advantage |
|---|---|
| Quant recruiting alignment | Direct match to current execution and low-latency roles at several target firms. |
| Memory control | Explicit layout, lifetime, allocation, and ownership decisions. |
| Hardware awareness | Mature support for cache, branch, SIMD, atomics, and hardware-counter analysis. |
| Networking | Boost.Asio exposes buffers, event loops, timers, and asynchronous I/O directly. |
| Domain modeling | Strong wrapper structs, `enum class`, `std::variant`, concepts, and value semantics. |
| Error handling | `std::expected` supports explicit failure paths without requiring exceptions in core logic. |
| Verification | Sanitizers, fuzzing, deterministic simulation, property tests, and TLA+. |
| Performance tooling | Google Benchmark, Linux `perf`, flamegraphs, compiler reports, and mature profilers. |

C++ does not provide compile-time memory and data-race safety. The design must
therefore minimize shared mutable state and make ownership visible. One logical
event loop owns each node's Raft state; network and storage adapters exchange
typed values and effects rather than exposing internal references.

Use one implementation language for the assessed system. Python may be used off
the correctness path for workload generation, statistical analysis, and plots.

## Recommended C++23 stack

### Language and build

- **C++23**, using value semantics, RAII, `std::span`, `std::variant`,
  `std::expected`, and concepts where they clarify contracts.
- **Clang** as the primary compiler and GCC as a compatibility build.
- **CMake presets + Ninja** for repeatable debug, sanitizer, release, and
  benchmark configurations.
- **Conan 2 or vcpkg**, selecting exactly one dependency manager.
- `clang-format`, `clang-tidy`, and strict compiler warnings in CI.
- Linux as the benchmark and deployment target; macOS may be used for ordinary
  development.
- Compiler version, standard-library version, optimization flags, LTO state, and
  target CPU recorded with every benchmark.

### Runtime and networking

- **Boost.Asio** for the production event loop, TCP, timers, signals, and
  asynchronous operations.
- **Protocol Buffers** for versioned peer and client message schemas.
- Explicit length-prefixed TCP framing with bounded message sizes.
- `std::span<std::byte>`, `std::vector<std::byte>`, and Asio buffer sequences for
  explicit buffer views and ownership.
- spdlog or a small structured `fmt`-based logger for event timelines.

Keep Boost.Asio outside the consensus and matching cores. Those cores must not
open sockets, sleep, read wall-clock time, or perform disk I/O. The production
adapter translates external events into explicit inputs and executes returned
effects.

### Consensus

- Implement the restricted Raft core directly from Ongaro and Ousterhout:
  fixed three-node membership, election, heartbeats, replication, conflict
  repair, majority commit, ordered application, and crash/restart recovery.
- Express it as a pure event-driven state machine:

```text
(raft_state, input_event)
    -> new_state
    + messages_to_send
    + records_to_persist
    + committed_entries_to_apply
```

- Represent dynamic role state with `std::variant`:

```cpp
using Role = std::variant<
    FollowerState,
    CandidateState,
    LeaderState
>;
```

- Use strong wrapper structs for `Term`, `LogIndex`, `NodeId`, `ClientId`, and
  request sequences so unrelated integers cannot be mixed accidentally.
- Use `std::variant` for input events and output effects, `std::visit` for
  dispatch, and `std::expected` for recoverable boundary errors.
- Use [eBay NuRaft](https://github.com/eBay/NuRaft) only as a reference or
  optional comparison implementation, not as a substitute for the assessed
  Raft core.
- Exclude membership changes, leases, pre-vote, leadership transfer, and
  optimized reads from the MVP.

### Deterministic matching and portfolio state

- One pure transition interface:

```text
(application_state, committed_command) -> new_state + deterministic_events
```

- Strong wrapper types for `PriceTicks`, `QuantityLots`, `OrderId`, `ClientId`,
  and sequence numbers.
- Integer ticks and lots; no floating point in replicated transitions.
- Limit orders with explicit price-time priority for the MVP.
- Ordering based on committed log index plus an intra-entry ordinal.
- Ordered containers or explicit sorting wherever iteration affects output.
- Checked or widened arithmetic with a documented overflow policy.
- Replicated deduplication keyed by `(client_id, request_sequence)`.
- No wall clock, randomness, locale, node identity, pointer address, unordered
  iteration, or asynchronous work inside `apply()`.

Begin with standard containers and measured correctness. Preallocated arenas,
intrusive structures, custom allocators, and lock-free queues are optional
optimizations only after profiling identifies a bottleneck.

### Persistence

- A project-owned append-only WAL using ordinary file I/O first.
- Record layout: magic, schema version, length, term, index, payload, and CRC32C.
- An RAII file-descriptor wrapper plus explicit handling of short writes,
  interruption, partial tails, checksum failures, truncation, and restart.
- A defined synchronization and acknowledgment contract.
- Durable `current_term`, `voted_for`, and log entries before protocol responses
  whose correctness depends on them.
- Snapshots only after log recovery works. Each snapshot includes its last
  included term/index, application state, deduplication state, and canonical
  digest.
- Named failpoints before and after append, sync, truncate, snapshot rename,
  apply, and client response.
- Byte-wise canonical encoding; never write raw C++ structs, padding bytes,
  pointers, or ABI-dependent layouts to disk.

Do not begin with memory-mapped persistence. POSIX `mmap` and `io_uring` are
valid post-MVP experiments, but neither automatically provides correct
durability.

### Testing and verification

- **GoogleTest/GoogleMock** for unit and component tests.
- **RapidCheck** for generated and minimized command sequences and invariants.
- **Clang libFuzzer** for decoders, WAL recovery, and malformed frames.
- Separate **AddressSanitizer**, **UndefinedBehaviorSanitizer**, and
  **ThreadSanitizer** CI configurations.
- **clang-tidy** for static analysis and policy enforcement.
- A seeded deterministic simulator with virtual time, stable event ordering,
  simulated network/disk, and crash/restart controls.
- A small **TLA+/PlusCal** model checked by TLC for election safety, log matching,
  leader completeness/state-machine safety, and client deduplication.

Every failure must print a replayable seed and fault history. Prefer value types,
RAII, standard containers, and explicit ownership. Raw pointers, manual memory
management, custom allocators, and lock-free structures require measured
justification, documented invariants, and isolated tests.

### Performance engineering

- **Google Benchmark** for matcher, codec, hashing, and WAL microbenchmarks.
- **HdrHistogram** for end-to-end p50/p95/p99/p99.9/max distributions.
- Linux `perf`, flamegraphs, and hardware counters.
- Allocation instrumentation for allocations per command.
- Documented CPU affinity and governor settings for controlled benchmarks.
- An open-loop load generator to avoid hiding overload through coordinated
  omission.

Do not present sanitizer-build timings as release performance. Avoid “zero
allocation,” “lock-free,” “zero copy,” and “nanosecond latency” as design
slogans. Establish a correct baseline, profile it, optimize one measured
bottleneck, and publish the before/after result and its tradeoff.

### Observability and artifacts

- Structured spdlog or `fmt` events with node, term, role, peer, log index,
  commit index, applied index, and stable request ID.
- Prometheus-format counters and histograms through prometheus-cpp;
  OpenTelemetry C++ export is optional.
- A terminal dashboard showing nodes, leader changes, replication lag, and state
  digests.
- JSON/CSV experiment artifacts and scripts or notebooks for plots.
- GitHub Actions checks for formatting, Clang/GCC builds, clang-tidy, unit and
  property tests, sanitizer jobs, fuzz smoke tests, deterministic scenarios,
  model checking, documentation, and benchmark compilation.

## Architecture

```text
bounded trace -> strategy/client -> leader gateway
                                  -> Boost.Asio/Protobuf adapter
                                  -> pure Raft transition core
                                     -> WAL and peer effects
                                     -> committed command stream
                                        -> pure matching/portfolio state

seeded simulator -> virtual clock/network/disk/process lifecycle
production shell -> Boost.Asio clock/TCP + POSIX files/process lifecycle

both environments execute the same protocol and application cores
```

Suggested project layout:

```text
include/
  domain/
  matching/
  raft/
  storage/
  transport/
src/
  domain/
  matching/
  raft/
  storage/
  transport/
  node/
  client/
  simulator/
tests/
benchmarks/
proto/
spec/tla/
experiments/
```

Each major component should be its own CMake target. One logical event loop owns
each node's mutable Raft state. Queues carry immutable inputs and effects rather
than expose consensus internals through shared references.

## Correctness experiment

At each applied index, compute canonical component digests for the committed
command prefix, orders, fills, portfolios, and client deduplication results.
Canonical encoding must sort keys and encode logical fields explicitly; never
hash raw memory, padding, addresses, or incidental container order.

Required scenarios:

1. No-fault golden replay.
2. Leader crash before local append.
3. Crash after local append but before majority replication.
4. Crash after majority commit but before client response.
5. Old leader isolated, new leader commits traffic, partition healed.
6. Delayed, reordered, duplicated, and dropped Raft messages.
7. Follower crash during append, restart, and catch-up.
8. Divergent uncommitted suffix repair.
9. Interrupted snapshot installation and retry.
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

1. Pure matching without logging or replication.
2. Single-node WAL-backed matching.
3. Three-node Raft with memory-only storage.
4. Three-node Raft with durable majority acknowledgment.

Cover add, cancel, partial/full fill, shallow/deep books, bursts, sustained load,
batch sizes, WAL policies, network RTT, and leader failure at each replication
boundary.

Report throughput and saturation; p50/p95/p99/p99.9/max latency; matcher time and
allocations; cycles, instructions, branch and LLC misses; serialization cost;
WAL sync distribution; failover interruption; and recovery/catch-up time.

Commit raw results, code revision, workload seed, hardware, kernel, compiler and
standard-library versions, compiler flags, warm-up policy, run count, and
confidence intervals. Treat outliers as observations to explain rather than
silently discard.

## High-value extensions

Only after the correctness matrix passes:

1. Preallocated slab or `std::pmr` arena for orders, measured against the
   standard-container baseline.
2. Bounded SPSC channel if profiling shows contention or scheduler interference.
3. FlatBuffers, Cap'n Proto, or a custom validated binary codec, with schema and
   compatibility analysis.
4. Memory-mapped or `io_uring` WAL with explicit crash tests.
5. Core pinning and cache-line layout experiments with hardware-counter evidence.
6. A NuRaft-backed comparison implementation and differential end-state tests.

Do not add DPDK, RDMA, FPGA work, Kubernetes, Kafka, or a polished web dashboard
until the core safety and measurement objectives are complete.

## Delivery sequence

1. C++23 ownership, RAII, value/move semantics, variants, error handling,
   concurrency, atomics, the memory model, and Asio lifetime rules.
2. Pure domain and matching state machine with golden/property tests.
3. Pure in-memory Raft core with deterministic unit scenarios.
4. Seeded simulator using the same Raft core.
5. Committed application, canonical digests, and client deduplication.
6. Checksummed WAL, restart replay, and storage failpoints.
7. Boost.Asio/Protocol Buffers production adapter and real three-process cluster.
8. Sanitizer, fuzzing, static-analysis, TLA+ model, and full failure matrix.
9. Performance baselines, raw artifacts, charts, and demo.
10. One measured extension if schedule permits.

## Resume-worthy outcome

> Built a C++23 three-node Raft-replicated limit-order-book simulator with a
> crash-consistent checksummed WAL, replicated request deduplication, and
> deterministic consensus and matching cores. Model-checked safety invariants in
> TLA+, reproduced crash and partition histories in a seeded simulator, and
> quantified consensus and failover costs using open-loop load generation, HDR
> histograms, sanitizers, and Linux hardware counters.

Add measured numbers only after the experiments exist.

## Evidence limits and primary references

Public job descriptions and talks show recruiting and communication priorities,
not complete private production stacks. The C++23 decision is an engineering
judgment based on this project's low-latency, distributed-systems, and recruiting
objectives, not a claim that every target team uses the same stack.

- [Raft paper and resources](https://raft.github.io/)
- [C++ Core Guidelines](https://isocpp.github.io/CppCoreGuidelines/CppCoreGuidelines)
- [cppreference](https://en.cppreference.com/)
- [Boost.Asio overview](https://www.boost.org/latest/doc/html/boost_asio/overview/basics.html)
- [Protocol Buffers C++ tutorial](https://protobuf.dev/getting-started/cpptutorial/)
- [eBay NuRaft](https://github.com/eBay/NuRaft)
- [GoogleTest](https://github.com/google/googletest)
- [RapidCheck](https://github.com/emil-e/rapidcheck)
- [Google Benchmark](https://github.com/google/benchmark)
- [LLVM AddressSanitizer](https://clang.llvm.org/docs/AddressSanitizer.html)
- [LLVM UndefinedBehaviorSanitizer](https://clang.llvm.org/docs/UndefinedBehaviorSanitizer.html)
- [LLVM ThreadSanitizer](https://clang.llvm.org/docs/ThreadSanitizer.html)
- [LLVM libFuzzer](https://llvm.org/docs/LibFuzzer.html)
- [clang-tidy](https://clang.llvm.org/extra/clang-tidy/)
- [HdrHistogram](https://github.com/HdrHistogram/HdrHistogram)
- [TLA+ examples](https://github.com/tlaplus/Examples)

When references or generative AI materially influence submitted work,
acknowledge them according to the course academic-honesty policy.
