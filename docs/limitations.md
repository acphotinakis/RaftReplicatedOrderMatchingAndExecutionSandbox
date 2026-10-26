# Limitations, Non-Goals, and Evidence Boundaries

> [!IMPORTANT]
> **Status: proposed analytical assessment.** This document defines the boundaries
> of the project's claims, underlying assumptions, and experimental conclusions.
> It explains what formal verification, simulation, and hardware measurements
> do and do not establish, and details the architectural consequences of
> non-goals and deferred functionality.
> All terminology and guarantees (`G1`–`G5`) are defined in
> [architecture.md §3–§4](architecture.md#3-vocabulary). Invariants and fault
> definitions reference [raft-design.md](raft-design.md) and [experiments.md](experiments.md).

---

## 1. Failure-Model Assumptions

The correctness guarantees (`G1`–`G4`) and conditional liveness (`G5`) claimed
by this system hold strictly within the boundaries of the classical
**Crash-Recovery (Fail-Stop / Crash-Restart)** failure model:

1. **Non-Byzantine Behavior:** Nodes, storage subsystems, and communication
   channels are strictly non-Byzantine. Nodes execute deterministic instructions
   faithfully without malicious compromise, unauthorized code execution, or
   covert state alteration.
2. **Crash-Stop and Crash-Restart:** A node may halt abruptly at any arbitrary
   execution step (power failure, OS panic, unhandled signal). A crashed node
   may restart under its identical node identifier $n \in \{n_1, n_2, n_3\}$
   with its synchronized non-volatile storage intact.
3. **Storage Integrity:** Synchronized disk blocks (`fsync` / `F_FULLFSYNC`) do
   not experience undetected physical media corruption or spontaneous bit-decay.
   Any partial write caused by a crash is confined to a trailing file suffix
   and is detectable via record-level CRC32C checksums.
4. **Asynchronous, Unreliable Network:** The network may arbitrarily delay,
   reorder, duplicate, or drop packets, and may partition nodes into isolated
   subsets. However, messages are not maliciously fabricated or silently
   corrupted in transit (network frame corruptions are caught by transport
   checksums).
5. **Partial Synchrony for Liveness Only:** Safety invariants (`R1`–`R11`, `A1`–`A8`)
   never depend on clock synchronization or message latency bounds. Liveness
   (`G5`) requires periods of partial synchrony where a majority of nodes can
   communicate with round-trip latencies substantially smaller than the randomized
   election timeout interval ($2 \Delta_{\text{net}} \ll E_{\min}$).

---

## 2. Explicit Non-Goals

To maintain clarity and protect the delivery of core consensus and determinism
proofs, the following capabilities are explicitly declared outside the scope
of this project:

- **Byzantine Fault Tolerance (BFT):** No mechanism exists to counter malicious
  proposals, forged signatures, or compromised validator quorums.
- **Production Exchange and Brokerage Connectivity:** No support for FIX/FAST
  protocols, binary exchange market-data feeds (e.g. Nasdaq ITCH/OUCH, CME MDP3),
  or live order routing to broker-dealers.
- **High-Frequency Trading (HFT) Performance Claims:** The project does not aim
  for sub-microsecond or nanosecond execution latencies. Claims of "ultra-low
  latency" or "zero-copy" architecture are non-goals.
- **Dynamic Cluster Membership and Reconfiguration:** No joint consensus, single-server
  membership changes, or dynamic additions/removals of cluster nodes.
- **Multi-Raft and Partition Sharding:** A single Raft consensus group replicates
  all entries. There is no horizontal scaling, key-range partitioning, or
  distributed multi-shard transaction coordination.
- **Complex Financial Instruments and Market Rules:** No margin accounts,
  leverage, short selling, borrowing costs, derivatives (options/futures),
  maker/taker fee structures, continuous auction opening crosses, or regulatory
  compliance reporting.
- **Production Security Infrastructure:** No mutual TLS (mTLS), public key
  infrastructure (PKI), role-based access control (RBAC), API key authentication,
  or encryption-at-rest.
- **Enterprise Distributed Middleware:** No external dependencies on Apache Kafka,
  Kubernetes, ZooKeeper, etcd, or cloud-native service meshes.

---

## 3. What the Evidence Does and Does Not Establish

The verification and experimental methodology employs multiple complementary
tools. Each tool covers specific failure surfaces but has distinct analytical
limits that must not be overstated:

| Verification Method | What It Establishes | What It Does NOT Establish |
|---|---|---|
| **Rust Type System & Memory Safety** | Eliminates broad classes of memory-safety vulnerabilities: null-pointer dereferences, buffer overflows, use-after-free, and data races between threads (guaranteed by `Send`/`Sync` ownership). | Does not prevent protocol deadlocks, logical livelocks, incorrect state transitions, algorithmic integer overflows, or race conditions across distributed network messages. |
| **TLA+ / PlusCal Formal Model Checking** | Exhaustively verifies that the abstract protocol specification satisfies safety invariants (`R1`–`R5`, `G1`, `G4`) across all reachable states within a bounded model (e.g., 3 nodes, max term 3, max entries 3). | Does not prove the implementation correct. The model abstracts away byte serialization, POSIX filesystem operations, torn writes, memory allocation, and concrete network socket behavior. |
| **Seeded Deterministic Simulation** | Proves that the concrete consensus and matching cores execute identically and safely across billions of permutations of message delays, packet drops, partitions, and crash-restart points under explicit pseudo-random seeds. | Does not simulate OS kernel scheduling anomalies, real NVMe hardware write-cache flushes, NUMA memory bus contention, hardware interrupt latency, or platform-specific syscall semantics. |
| **CRC32C Record Checksums** | Detects torn writes, incomplete tail writes caused by abrupt power interruptions, and accidental byte corruption within serialized disk blocks. | Does not provide cryptographic tamper-proofing against intentional modification, nor does it repair damaged mid-log records (corruption results in fail-stop). |
| **Component State Digests (SHA-256)** | Formally verifies bit-for-bit application state parity ($\Delta D = 0$) across independent replica memory spaces at identical committed and applied log indexes. | Does not verify that the matching engine logic aligns with real-world financial market microstructure or that execution prices represent profitable or realistic trading behavior. |

---

## 4. Delivery Risks and Systemic Limits

### 4.1 Fixed Membership Boundaries
Because dynamic membership is excluded, the loss of two nodes permanently halts
cluster progress (`F14`). Furthermore, if a node's physical persistent storage
is permanently destroyed or wiped, **it cannot be replaced with a fresh blank node**.
Introducing a blank node under the same identifier would violate quorum safety:
the new node might vote for a candidate lacking previously committed entries.
Re-provisioning requires manual cold backup restoration or cluster re-genesis.

### 4.2 Client Retry and Idempotency Assumptions
Effectively-once logical execution (`G4`) relies strictly on well-behaved clients:
- Clients must maintain monotonically increasing request sequence numbers
  $s = s_{\text{prev}} + 1$.
- In the event of network disconnection or leader failover, clients must retry
  the **identical byte payload** with the identical idempotency key $(c, s)$.
- Retrying with a mutated payload or skipped sequence number results in
  deterministic rejection (`KeyConflict` or `SequenceGap`), leaving the client
  unable to proceed without session reset.
- At-least-once transport is assumed; true "exactly-once delivery" across an
  unreliable network is physically impossible under the Two Generals' problem.

### 4.3 Durability Scope and Storage Realities
The guarantee that an acknowledged command will survive future crashes (`G3`)
is entirely contingent on the underlying operating system and storage controller
honoring persistence primitives:
- On Linux systems, `fdatasync` must flush volatile drive controller caches to
  stable non-volatile media. If hardware drives silently ignore flush commands
  (a known issue on consumer-grade SSDs), power-loss durability is void.
- On macOS development environments, standard `fsync` does not flush drive
  write caches; `fcntl(fd, F_FULLFSYNC)` is required (`D-15`).
- The memory-only baseline `B3` explicitly bypasses disk synchronization; it
  provides zero crash durability and cannot be cited as satisfying `G3`.

### 4.4 Performance Generalizability
All performance measurements obtained in this sandbox represent specific,
controlled synthetic workloads:
- Performance numbers gathered on single-host loopback network deployments
  reflect zero physical wire latency, zero switch queuing, and ideal network
  conditions.
- Host-level contention (co-locating 3 node processes and the benchmark load
  generator on a single CPU) introduces scheduling jitter and cache eviction
  artifacts that differ substantially from a bare-metal 3-server rack configuration.
- Throughput and latency figures from synthetic Poisson or constant-rate streams
  cannot be extrapolated to real market conditions characterized by extreme
  non-linear volatility bursts and correlated order cancellations.

### 4.5 Market and Execution Realism
The deterministic execution engine uses a simplified order-matching model:
- Fills against external quotes assume instantaneous execution of resting liquidity
  without queue competition or market impact.
- The 5/20 SMA crossover strategy and 1-minute OHLCV bar workloads do not reflect
  intrabar price action, spread fluctuations, or order-book queue depletion.
- Because order cancellation (`CancelOrder`) is deferred in the MVP, orders rest
  indefinitely until matched or expired. Cash and position reservations remain
  locked, artificially restricting trading capacity over prolonged traces.

### 4.6 Deferred Functionality and Architectural Consequences

The following table summarizes the operational consequences of features deferred
from the MVP baseline:

| Deferred Feature | Architectural Consequence | Operational Impact |
|---|---|---|
| **Log Compaction & Snapshots** | The WAL grows indefinitely with every committed command. | Replicas require increasing disk space; restarted nodes must replay the entire log history from index 1, extending catch-up time $T_{\text{catchup}}$ proportionally to trace length. |
| **Pre-Vote Protocol** | Rejoining partitioned nodes with stale logs but inflated terms trigger disruptive elections. | Causes transient leader step-downs and brief cluster commitment pauses upon partition healing (`F08`, `F09`). |
| **Read-Index / Lease Reads** | All read operations must be routed through the Raft log as committed commands or restricted to non-linearizable inspection. | Read throughput is bound to write throughput; inspection queries at followers may return slightly stale state. |
| **Order Cancellation** | Clients cannot withdraw resting limit orders. | Portfolios accumulate locked reservations; strategy cannot adapt to rapidly moving markets by pulling stale bids/asks. |
| **Batched I/O & Pipeline Replication** | Each `AppendEntries` is flushed synchronously before sending responses. | Latency is strictly lower-bounded by sequential disk sync times ($L_{\text{sync}}$); throughput cannot saturate network bandwidth. |
