# Experiments and Evaluation Plan

> [!IMPORTANT]
> **Status: proposed experimental design. No experiments have been conducted yet.**
> No consensus, matching, storage, or benchmarking implementations currently exist
> (see [architecture.md §11](architecture.md#11-repository-mapping-and-implementation-status)).
> This document specifies the hypotheses, test workloads, baselines, fault matrix,
> metrics, seed replay mechanisms, and artifact schema needed to evaluate the
> project claims once implemented. All references to system guarantees (`G1`–`G5`),
> entry lifecycle states, and decisions (`D-01`–`D-16`) refer to
> [architecture.md §3, §4, §12](architecture.md#3-vocabulary). Invariants `R1`–`R11`
> and `A1`–`A8` are defined in [raft-design.md §11](raft-design.md#11-safety-invariants)
> and [execution-model.md §7](execution-model.md#7-portfolio-updates).

---

## 1. Research Questions and Hypotheses

The project evaluates one overarching question formulated in [start/project.md](start/project.md):
*Can replicated-log agreement preserve identical committed order, fill, and portfolio state after leader failure?*

To answer this comprehensively, the evaluation addresses six sub-questions (`RQ1`–`RQ6`):

- **RQ1 (State Consistency):** Does Raft log agreement produce identical trading application state, rather than merely identical logs?
  - *Hypothesis H1:* For any set of replicas that have executed a common committed prefix through applied index $i$, their computed state digests ($D_{\text{book}}$, $D_{\text{orders}}$, $D_{\text{fills}}$, $D_{\text{accounts}}$, $D_{\text{sessions}}$, $D_{\text{market}}$) will match identically ($\Delta D = 0$).
- **RQ2 (Deterministic Execution Bounds):** Which domain modeling constraints prevent divergence across replicas?
  - *Hypothesis H2:* Removing wall-clock time, floating-point arithmetic, node IDs, and non-deterministic iteration from the application transition function (`apply`) guarantees zero divergence across heterogeneous execution schedules.
- **RQ3 (Fault Boundary Safety):** Are safety and effectively-once execution preserved at every failure boundary?
  - *Hypothesis H3:* Injecting leader crash, message loss, or transport disconnects before append, during append, after replication, or after commitment will never lose an acknowledged command, never violate portfolio conservation, and never execute a retried order twice.
- **RQ4 (Crash Recovery Invariants):** Can crashed or partitioned replicas recover and catch up without violating Raft or application invariants?
  - *Hypothesis H4:* A recovering replica will truncate divergent uncommitted log tails, replay the committed log prefix from durable storage or leader catch-up, and attain an identical state digest to active nodes at index $i$.
- **RQ5 (Liveness & Availability):** Under what network and timeout conditions does the cluster maintain or regain forward progress?
  - *Hypothesis H5:* When at least two of the three nodes can exchange messages within broadcast time $\Delta < E_{\min}$, the cluster elects exactly one leader per term and resumes commitment within $T \le 2 \times E_{\max}$.
- **RQ6 (Consensus and Durability Overhead):** What are the isolated incremental latency and throughput costs of matching, WAL persistence, and consensus replication?
  - *Hypothesis H6:* A single-node in-memory matcher will achieve orders-of-magnitude lower latency than WAL-backed or Raft-replicated execution; storage synchronization (`fsync` / `F_FULLFSYNC`) will dominate end-to-end commit latency over LAN round-trip replication.

---

## 2. Workload Definitions

All experiments utilize reproducible, bounded input traces. No live or interactive market streaming is allowed.

### 2.1 Trace Characteristics

| Workload ID | Name | Source Data | Volume | Purpose | Target RQ |
|---|---|---|---|---|---|
| **W1** | Single-Session SPY OHLCV | Alpaca 1-min bars (e.g. `2024-01-03`, 390 bars) converted upstream to quotes ([execution-model.md §5.3](execution-model.md#53-external-liquidity-from-market-events)) | 390 bars ($\approx 780$ synthetic quotes) + $\approx 20\text{--}50$ strategy orders | MVP correctness baseline, deterministic replay, basic failover | RQ1, RQ2, RQ3, RQ4 |
| **W2** | SPY Historical Quotes | Alpaca raw NBBO quote updates sampled from historical feed | $10^4$ to $10^5$ quote updates | Stress spread-aware book updates and resting queue matching | RQ1, RQ2, RQ6 |
| **W3** | Multi-Client Synthetic Stream | Synthetic generator emitting Poisson/burst limit orders across 10 client IDs | $10^5$ limit orders (bids/asks with uniform tick distribution) | Stress price-time priority, FIFO queue depth, client deduplication | RQ2, RQ3, RQ6 |
| **W4** | Open-Loop Saturation Benchmark | High-rate pre-generated order stream at controlled inter-arrival intervals | $10^6$ operations | Determine saturation knee, queue buildup, and tail latency | RQ6 |

### 2.2 Trace Normalization & Manifest Schema

All raw trace files are preprocessed into an immutable JSON Lines format before entering the cluster. Float values are converted to 64-bit integer price ticks ($10^{-4}$ USD) and integer lot quantities ($1 \text{ lot} = 1 \text{ share}$) per decision `D-08`.

Each trace is paired with a cryptographic manifest `trace_manifest.json`:
```json
{
  "manifest_version": "1.0.0",
  "trace_id": "SPY-2024-01-03-OHLCV-v1",
  "source": "Alpaca Historical Bars API v2",
  "symbol": "SPY",
  "scale_exponent": -4,
  "start_time": "2024-01-03T09:30:00Z",
  "end_time": "2024-01-03T16:00:00Z",
  "event_count": 780,
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "generated_at": "2026-10-25T12:00:00Z"
}
```

---

## 3. Metrics and Measurement Protocols

### 3.1 Primary Metrics

1. **State Divergence ($\Delta D$):**
   $$\Delta D(i) = \sum_{c \in \text{Components}} \mathbb{I}\left(D_c^{(n_a)}(i) \neq D_c^{(n_b)}(i)\right)$$
   Evaluated across all active/recovered nodes at common applied index $i$. Acceptable threshold: **strictly 0**.
2. **End-to-End Latency ($L_{\text{e2e}}$):**
   Wall-clock duration from client send intent timestamp $t_{\text{submit}}$ to acknowledgment decode $t_{\text{ack}}$. Recorded via HdrHistogram covering p50, p90, p99, p99.9, and Max.
3. **Commit Latency ($L_{\text{commit}}$):**
   Time elapsed from leader entry insertion into local WAL memory buffer to quorum synchronization and `commit_index` advancement.
4. **Synchronization Latency ($L_{\text{sync}}$):**
   Time spent inside the platform storage flush call (`fdatasync`/`F_FULLFSYNC`).
5. **Failover Downtime ($T_{\text{failover}}$):**
   Time elapsed between leader termination/isolation and the successful commitment of the first new entry under the successor leader.
6. **Recovery Catch-up Duration ($T_{\text{catchup}}$):**
   Time taken by a restarted follower from process invocation until its local `last_applied` reaches the cluster's active `commit_index`.
7. **Throughput ($X$):**
   Committed operations applied per second during sustained steady-state execution.

### 3.2 System Profiling Counters

For baseline performance comparisons (`B1`–`B4`), Linux `perf` captures:
- Instructions Per Cycle (IPC), branch mispredictions (`branch-misses`), LLC cache misses (`LLC-load-misses`), context switches, and page faults.
- Heap allocations per command (tracked via custom memory-allocator profiling hooks).

---

## 4. Baselines

To separate the intrinsic costs of matching, storage durability, and network consensus, performance is compared against four progressive architectural baselines executing the identical workload W4:

```
[ B1: In-Memory Matcher ]
        │  + WAL Append & fsync
        ▼
[ B2: Single-Node Durable WAL ]
        │  + 3-Node Raft Consensus (No fsync)
        ▼
[ B3: 3-Node Raft In-Memory ]
        │  + Quorum Durable Storage (fsync)
        ▼
[ B4: 3-Node Raft Full Quorum Durability (Production Sandbox) ]
```

| Baseline ID | Name | Architecture Description | Purpose |
|---|---|---|---|
| **B1** | Pure In-Memory Matcher | Single-process, single-threaded matching core. No Raft, no disk I/O, no network serialization. | Measures pure algorithm performance and memory footprint limit. |
| **B2** | Single-Node Durable WAL | Single-node matcher backed by the project-owned append-only WAL. Calls `fdatasync` per batch/command. | Measures the isolated cost of filesystem durability without consensus. |
| **B3** | Three-Node Raft (Memory-Only) | Full Raft consensus protocol across 3 nodes over loopback/TCP; storage operations bypass `fsync`. | Isolates RPC networking, serialization, and quorum replication overheads from disk I/O. |
| **B4** | Three-Node Raft (Durable Quorum) | Full production sandbox: 3 nodes, Raft consensus, project-owned WAL with synchronous storage flushing. | Measures the complete cost of distributed, crash-consistent state-machine replication. |

---

## 5. Fault Matrix

The fault matrix reconciles the test cases from `rust-tech-scope.md`, `rust-quant-tech-stack-research.md`, and the student guide pitfalls checklist ([raft-design.md §14](raft-design.md#14-implementation-pitfalls-checklist)).

Every scenario defines an explicit injection point, expected behavior, checked invariants, and captured evidence.

| Scenario ID | Name | Injection Point | Expected Behavior | Invariant Checked | Captured Artifact Evidence |
|---|---|---|---|---|---|
| **F01** | Golden Reference Replay | Clean run; no injected faults. | All 3 nodes process workload W1 to completion; identical state. | R1–R11, A1–A8, G1, G2 | Matching trace digest logs from `n1`, `n2`, `n3`. |
| **F02** | Crash Before Append | Leader crashes immediately upon receiving client proposal before writing to local WAL buffer. | Proposal lost; client times out and retries on new leader; executed once. | G3, G4, A8 | Client retry trace; single entry logged across all cluster nodes. |
| **F03** | Crash After Write, Before Sync | Leader writes entry to OS buffer, crashes before `fsync` completes. | Recovered leader truncates un-synced tail on restart; command re-proposed by client on successor. | R8, R11, G4 | Crash recovery log showing torn-tail truncation at restart. |
| **F04** | Crash After Sync, Before Replication | Leader synchronizes entry locally, crashes before emitting `AppendEntries` to followers. | Entry uncommitted; successor leader overwrites index or commits if re-elected; client retries safely. | R3, R4, G1, G4 | Leader election logs; conflict repair report. |
| **F05** | Crash After Majority Replication, Before Commit Response | Leader synchronizes, replicates to 1 follower (quorum reached), but crashes before updating volatile `commit_index` or replying. | Successor leader with up-to-date log commits entry (via `Noop`); client retries, gets cached outcome. | R4, R9, G3, G4 | Client retry log with `duplicate=true`; identical state hashes. |
| **F06** | Crash After Commit & Apply, Before Response | Leader commits and applies entry, updates session table, crashes before sending client response. | Client retries key `(c, s)` against new leader; new leader replicates retry, `apply` recognizes duplicate and returns cached result. | G4, A8 | Session table dump showing `last_seq=s`; no double portfolio deduction. |
| **F07** | Network Response Loss | RPC layer drops leader response to client; leader remains alive. | Client retransmits identical payload; leader returns cached outcome without re-executing matching. | G4, A8 | Network dropped-packet counter; client receipt of original result. |
| **F08** | Partitioned Stale Leader Reintegration | Leader partitioned into minority `{n1}`; `{n2, n3}` elect new leader and commit new entries; partition heals. | Old leader rejects writes; steps down on receiving higher term; truncates uncommitted divergent tail and catches up. | R1, R3, R6, G1 | Term step-down log; WAL truncation record on `n1`. |
| **F09** | Stale Candidate Re-election Disruptions | Follower partitioned, repeatedly increments term, rejoins cluster. | Stale candidate rejected if log behind; with pre-vote deferred, high term forces election, but safe leader elected. | R1, R4, R6 | Election transcript verifying safety across term jump. |
| **F10** | Unreliable Transport (Drop, Delay, Reorder) | Network adapter randomly drops (5%), delays (up to 200ms), and reorders `AppendEntries`. | Heartbeats recover dropped packets; reordered entries handled idempotently; logs converge. | R3, R10, G1 | Packet reordering statistics; pairwise log parity verified at convergence. |
| **F11** | Follower Crash and Catch-Up | Follower crashes during active stream; offline for 100 entries; restarts. | Follower restarts from WAL, rejoins as follower, receives backfilled entries from leader, catches up. | R8, R10, G2 | Follower catch-up metric $T_{\text{catchup}}$; state digest verification. |
| **F12** | Divergent Uncommitted Suffix Repair | Split leadership across rapid terms creates conflicting uncommitted logs on 2 nodes. | New stable leader forces truncation of conflicting indices via `prev_log_index`/`prev_log_term` backtracking. | R3, R4, G1 | Backtracking trace log; truncation offsets. |
| **F13** | Torn Tail Recovery | Node crash mid-write leaves incomplete trailing bytes and invalid CRC at EOF. | Recovery parser identifies invalid trailing CRC, truncates corrupted segment at clean boundary, resumes safely. | R8, R11 | Recovery audit log confirming valid partial-tail truncation. |
| **F14** | Quorum Loss (2 Nodes Dead) | 2 of 3 nodes killed simultaneously. | Remaining node halts progress; no client proposals commit; state machine freezes safely until quorum returns. | G1, G5 | Rejection of proposals with `Retryable(Unavailable)`. |
| **F15** | Full Cluster Cold Restart | Entire cluster powered off after committing index $N$; restarted simultaneously. | All 3 nodes load WALs, elect leader, append and commit `Noop`, restore $last\_applied = N$. | R6, R8, G1, G2 | Cold restart logs; state hash match at index $N+1$. |
| **F16** | Mid-Log Corruption Detection | Bit-flip injected into middle of historical synchronized WAL segment. | Node fails CRC check on startup; refuses to boot and halts safely rather than serving corrupted state. | R8, R11 | Process panic log with explicit checksum violation report. |
| **F17** | Interrupted Snapshot Installation *(Deferred)* | Snapshot transfer interrupted by node crash mid-stream. | Partially transferred snapshot discarded; retry restarts transfer cleanly without corrupting base state. | R9, G1 | Snapshot staging directory cleanup trace. |

---

## 6. Acceptance Criteria

An experimental run is deemed passing (`VERDICT: PASS`) if and only if all of the following conditions are met:

1. **State Parity Criterion:** At every common applied index $i \in [1, N]$, the component digests across all active and recovered nodes are bit-for-bit identical:
   $$D_{\text{state}}^{(A)}(i) = D_{\text{state}}^{(B)}(i) = D_{\text{state}}^{(C)}(i)$$
2. **Zero Invariant Violations:** No run produces a violation of Raft safety invariants `R1`–`R11` or financial invariants `A1`–`A8`.
3. **No Duplicate Execution:** Under any combination of leader crash and network retry (scenarios `F05`, `F06`, `F07`), the client's observed portfolio reflects exactly one application of each logical order request.
4. **Clean Recovery:** Any crashed node restarts, validates its WAL CRC, catches up to the current cluster leader, and participates in future quorums without human intervention.
5. **No Undefined Timing Behaviors:** The cluster successfully elects a leader and processes commands within bounded failover downtime ($T_{\text{failover}} \le 2 \times E_{\max}$) in all scenarios where a communicating majority is restored.

---

## 7. Experimental Controls and Methodology

### 7.1 Seed Replay

Deterministic simulation and fault injection are parameterized by an explicit 64-bit integer seed (`RNG_SEED`). 
- The pseudo-random number generator drives:
  - Randomized election timeout intervals within $[E_{\min}, E_{\max}]$.
  - Network fault scheduling (packet drop, delay, jitter).
  - Storage failpoint triggers and crash timings.
- *Replication Contract:* Running the deterministic simulator with the identical `RNG_SEED`, binary revision, and workload trace must produce an identical event execution trace down to the instruction/event sequence.
- Any test failure must log:
  ```text
  [FAIL] Invariant R5 violated at applied_index=142.
  Reproduce with: cargo test --test simulator -- --seed 0xDEADBEEF42
  ```

### 7.2 Comparison at Common Applied Indexes

Nodes in a distributed cluster advance their volatile `last_applied` index asynchronously. Comparing node states at identical wall-clock instants produces false-positive divergence reports.

**Evaluation Protocol:**
1. Each node emits an append-only digest log record upon applying index $i$:
   $$\text{Record}(i) = \langle i, \text{term}, D_{\text{log}}(i), D_{\text{state}}(i) \rangle$$
2. The experiment validation checker consumes the digest logs offline or out-of-band.
3. Comparisons are performed strictly at **common applied indexes**:
   $$I_{\text{common}} = \{ i \mid i \le \min(last\_applied_{n1}, last\_applied_{n2}, last\_applied_{n3}) \}$$
4. For all $i \in I_{\text{common}}$, verify that $D_{\text{state}}(i)$ is identical across all participating nodes.

### 7.3 Open-Loop Load Generation

To prevent the **coordinated omission** problem common in closed-loop benchmarks (where client delays during cluster pauses artificially suppress tail latency measurements), all throughput and saturation tests (`W4`, `RQ6`) use an open-loop load driver:

- Orders are scheduled at timestamps governed by a Poisson or constant-rate arrival process:
  $$t_{\text{scheduled}}(k) = t_0 + k \times \Delta t$$
- Latency is computed relative to the *scheduled* arrival time, not the actual transmission time:
  $$L(k) = t_{\text{ack}}(k) - t_{\text{scheduled}}(k)$$
- If the cluster stalls during a leader election, the open-loop driver continues to generate load, accurately exposing the resulting queue buildup and latency spikes in the HDR histogram.

### 7.4 Benchmark Controls & Environment Rigor

Performance evaluation runs (`B1`–`B4`) enforce the following hardware and kernel controls:
- **CPU Pinning:** Nodes, benchmark drivers, and disk sync threads are pinned to dedicated physical CPU cores using `taskset` / `pthread_setaffinity_np`.
- **Frequency Governor:** Set to `performance` mode; Intel Turbo Boost / AMD Core Performance Boost disabled to eliminate frequency throttling jitter.
- **Warm-Up:** Each benchmark executes a discarded warm-up phase ($10^4$ operations) to populate processor caches and branch history tables before measurement begins.
- **Confidence Intervals:** Every latency and throughput experiment is repeated across 10 independent trials. Results report median, p99, and 95% bootstrap confidence intervals.

---

## 8. Artifact Metadata Schema

Every test execution and benchmark run emits a machine-readable JSON summary alongside its raw data logs in the `results/` directory:

```json
{
  "run_id": "run-20261025-f05-seed42",
  "timestamp": "2026-10-25T20:30:00Z",
  "git_commit": "791de25",
  "git_dirty": false,
  "config": {
    "cluster_size": 3,
    "election_timeout_ms": [300, 600],
    "heartbeat_interval_ms": 75,
    "storage_mode": "DurableWAL",
    "fsync_primitive": "fdatasync"
  },
  "workload": {
    "manifest_id": "SPY-2024-01-03-OHLCV-v1",
    "total_commands": 830
  },
  "experiment": {
    "scenario_id": "F05",
    "seed": "0x000000000000002A",
    "fault_injected": "LeaderCrashAfterMajorityReplication"
  },
  "environment": {
    "os": "Linux 6.8.0-45-generic x86_64",
    "cpu": "AMD EPYC 7763 64-Core Processor",
    "rustc_version": "rustc 1.85.0 (2024-edition)"
  },
  "validation": {
    "verdict": "PASS",
    "max_common_index": 830,
    "invariants_checked": ["R1", "R2", "R3", "R4", "R5", "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8"],
    "divergences_detected": 0
  },
  "metrics": {
    "e2e_latency_micros": {
      "p50": 1250,
      "p90": 1820,
      "p99": 4510,
      "max": 14200
    },
    "failover_downtime_ms": 485,
    "throughput_ops_per_sec": 780.5
  }
}
```

---

## 9. MVP Tests versus Tests Requiring Deferred Features

To maintain clear project scoping and avoid conflating core consensus correctness with deferred extensions, tests are separated into two distinct execution tiers:

### 9.1 MVP Test Suite (Required for Baseline Completion)

The MVP test suite directly maps to the existing empty test harness files:
- `tests/raft_election.rs`: Covers `F01`, `F09`, invariants `R1`, `R6`, `R7`, `G5`.
- `tests/raft_replication.rs`: Covers `F01`, `F08`, `F10`, `F12`, invariants `R2`, `R3`, `R4`, `R8`, `R9`, `R10`, `G1`.
- `tests/raft_recovery.rs`: Covers `F03`, `F11`, `F13`, `F15`, `F16`, invariants `R8`, `R11`, `G3`.
- `tests/execution_determinism.rs`: Covers `W1`, invariants `A1`–`A8`, `G2`, `G4`.
- `tests/end_to_end.rs`: Covers `F02`, `F04`, `F05`, `F06`, `F07`, client retry deduplication, full cluster failover.

### 9.2 Tests Requiring Deferred Features (Post-MVP)

These scenarios require architectural extensions that are explicitly deferred from the initial implementation:
- **Scenario `F17` (Snapshot Installation & Recovery):** Requires Raft compaction, state snapshot serialization, and `InstallSnapshot` RPC implementation.
- **Order Cancellation Matrix:** Tests verifying that `CancelOrder` preserves price-time queue consistency and returns reserved cash across leader failures (requires order cancellation command implementation).
- **Multi-Symbol Cross-Sectional Arbitrage:** Stressing independent order books and shared cross-asset portfolio margins (requires multi-asset data pipelines).
- **Asynchronous Pipelining & Batched Disk I/O:** Testing high-throughput saturation with un-flushed pipelined `AppendEntries` (requires pipelined network adapter).
