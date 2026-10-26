# Alpaca API Architecture Comparison for a Raft-Replicated Trading Sandbox

My recommendation is to use SPY 1-minute OHLCV bars as the baseline, then introduce historical quotes and trades as separate experimental configurations. This allows you to measure how increasing market-data granularity changes execution realism, consensus throughput, log volume, and recovery performance.

There is an important distinction: your project is fundamentally about distributed consensus and deterministic execution, not maximizing trading profitability. The market-data APIs should support that research rather than unnecessarily complicate it.

## 1. Comparing the possible API combinations

Ratings are architectural assessments, not measured benchmarks.

| Configuration                  | Execution realism | Complexity | Raft workload |
| ------------------------------ | ----------------- | ---------- | ------------- |
| A. OHLCV only                  | Low               | Low        | Low           |
| B. OHLCV + Quotes              | Medium            | Medium     | Medium–High   |
| C. OHLCV + Trades              | Low–Medium        | Medium     | Medium–High   |
| D. Trades + Quotes             | Medium–High       | High       | High          |
| E. OHLCV + Trades + Quotes     | Medium–High       | Very High  | High          |
| F. Multi-symbol OHLCV          | Low               | Medium     | Low–Medium    |
| G. Multi-symbol OHLCV + Quotes | Medium            | High       | Medium–High   |

The Raft workload depends on what you actually replicate. You can process millions of market events outside Raft while committing only strategy orders, or replicate every market event and substantially increase consensus traffic.

## 2. What each API actually contributes

Historical OHLCV bars

Open, high, low, close, volume, and sometimes trade count/VWAP. Best for momentum, mean reversion, volatility, and portfolio strategies. Bars summarize trading activity but lose intrabar event order.

Historical quotes

Bid/ask prices and displayed sizes. Best for spread-aware execution, liquidity checks, and simulated transaction costs. Quotes do not provide full depth or actual queue position.

Historical trades

Individual executions with timestamps, prices, quantities, and conditions. Best for trade-flow signals, event-level replay, and studying high-frequency bursts. Trades alone do not identify the current executable bid or ask.

A crucial point about SPY: it is an excellent choice because of its liquidity and extensive trading history, but 1-minute bars still give you roughly 390 regular-session observations per full trading day. Higher trading volume does not produce more one-minute bars. To increase the number of replay events, use finer-resolution bars, trades, quotes, more sessions, or more symbols.

## 3. Option A — SPY OHLCV only

Simplest implementation

This configuration uses only Alpaca's historical bars API.

```
GET /v2/stocks/SPY/bars
    ?timeframe=1Min
    &start=2026-10-01
    &end=2026-10-02
    &feed=iex
```

Alpaca supports minute, hourly, daily, weekly, and monthly aggregations. The documented stock-bars endpoint starts at one-minute resolution; it does not provide native one-second bars.&#x20; 

[image](https://www.google.com/s2/favicons?domain=https://docs.alpaca.markets\&sz=32)

docs.alpaca.markets

### Architecture

SPY 1-minute OHLCV

Bar-driven strategy (momentum / mean reversion)

Simulated orders → Raft log

Deterministic bar-fill engine + portfolio

Example: Calculate a 5-bar moving average and a 20-bar moving average. When the short moving average crosses above the long moving average, submit a simulated buy order.

Advantages: Very easy to implement, compact data, straightforward deterministic replay, and simple validation.

Disadvantages: You don't know the true bid/ask spread or the order of prices within a candle. A fill at the candle's high, low, or close may be unrealistic.

For a defensible simulator, generate a signal after bar \\(t\\) closes and execute no earlier than bar \\(t+1\\), with a clearly defined fill rule. Otherwise you risk look-ahead bias.

Research impact: Best for proving Raft correctness, but not particularly strong for demonstrating market-microstructure-aware execution.

## 4. Option B — SPY OHLCV + historical quotes

Best overall balance

This is the combination I would prioritize for your project.

The historical bars API generates trading signals, while the historical quotes API determines simulated execution prices and available displayed liquidity.

### Architecture

OHLCV

Strategy signals

Quotes

Execution pricing

Synchronized Replay Scheduler

Market time + deterministic event ordering

Strategy → Raft → Execution Engine

Committed order + quote-based fill + portfolio update

Example:

* SPY 1-minute bar closes at $670.20.
* Your momentum strategy generates a BUY order.
* The next eligible historical quote has a bid of $670.19 and an ask of $670.22.
* Your simulated order fills at $670.22, subject to your liquidity and latency assumptions.

This separates signal generation from execution modeling, which is a common and useful architecture in trading systems.

It also gives you additional research metrics:

* Bid-ask spread at execution.
* Simulated slippage versus bar-close price.
* Available displayed size versus order quantity.
* Consensus latency and its impact on the next eligible execution quote.
* Portfolio divergence after leader failures.

However, you must explicitly model the difference between historical market time and simulated processing time. If Raft takes 50 milliseconds to commit an order, you can either use a deterministic virtual-clock model or a controlled latency experiment; you cannot simply apply different wall-clock timing on each replica.

## 5. Option C — SPY OHLCV + historical trades

This combines bar-driven strategies with individual executed trades.

OHLCV is still used to generate signals, but historical trades provide event-level information about actual market activity.

For example, you could detect unusually high buying pressure using trade-price changes and volume, or use trade arrivals to trigger a simulated limit-order execution model.

The difficulty is that a historical trade at $670.25 does not prove that your simulated order could have filled at $670.25.

Without quotes, you cannot reliably know the executable spread. Without order-book depth, you cannot determine queue priority.

This configuration is useful for studying:

* Trade-arrival bursts.
* Event-driven strategies.
* Trade-based volume and volatility signals.
* Higher-frequency replay and Raft throughput.

It is less suitable than OHLCV + Quotes for modeling market-order execution.

One valuable experiment would compare the same Raft implementation under a low-frequency bar-driven workload and a high-frequency trade-event workload.

## 6. Option D — SPY historical trades + quotes

This option removes OHLCV from the primary replay data and builds an event-driven system.

Trades update market activity, while quotes update executable bid/ask state. A strategy can consume both streams and produce orders.

You can still derive OHLCV bars locally from the historical trades if your strategy needs them, avoiding a third API. But locally derived bars must use appropriate trade-condition filtering, session boundaries, and the same source feed; they may not match Alpaca's official bars exactly.

This is the best configuration for high-frequency event replay and execution simulation, but it introduces more complexity:

* Interleaving trades and quotes with identical or near-identical timestamps.
* Maintaining the latest quote state.
* Avoiding future-data leakage.
* Filtering trade and quote conditions.
* Managing substantially larger traces.
* Modeling stale or crossed quotes.
* Preserving deterministic ordering through failure and recovery.

If your objective is to stress Raft with many operations, this is particularly interesting. If your objective is to finish a reliable course project, it may be more than you need.

## 7. Option E — SPY OHLCV + trades + quotes

Using all three APIs gives you the broadest market-data representation, but the greatest amount of integration work.

OHLCV

Indicators

Trades

Activity

Quotes

Liquidity

Unified historical event stream

Strategy + Matching + Raft Replication

The main advantage is that you can separately analyze strategy performance, liquidity, and trade flow.

The main disadvantage is redundant information. OHLCV bars are aggregates of trading activity already represented in the trades stream. That means you must avoid treating a bar and its underlying trades as independent volume or execution events.

I would reserve this configuration for an extension, especially if you want to test whether different data representations or replay frequencies affect consensus performance.

## 8. What changes when you use multiple symbols?

Multiple symbols fundamentally change the project from a single-instrument execution simulator into a portfolio-level distributed trading simulator.

With SPY alone, you can test deterministic execution, leader election, log replication, and crash recovery. With multiple symbols, you can additionally test whether replicas maintain identical portfolio state when strategies make decisions across instruments.

### Three multi-symbol approaches

| Approach                     | Example universe | Strategy                                         | Additional complexity |
| ---------------------------- | ---------------- | ------------------------------------------------ | --------------------- |
| Multi-symbol OHLCV           | SPY, QQQ, IWM    | Relative momentum                                | Low–Medium            |
| Multi-symbol OHLCV + Quotes  | SPY, QQQ, IWM    | Cross-sectional momentum with spread-aware fills | Medium–High           |
| Multi-symbol Trades + Quotes | SPY, QQQ, IWM    | Event-driven relative-value strategy             | Very High             |

### Example: Cross-sectional momentum

Suppose the strategy ranks five liquid ETFs by their previous 20-minute returns.

Illustrative 20-minute returns

-0.3%-0.05%0.2%0.5%QQQSPYIWMDIAXLK

Hypothetical data, not actual market returns.

The strategy could buy the highest-ranked symbol and sell an existing position in the lowest-ranked symbol.

The Raft log now contains orders for different symbols, and every replica maintains a shared cash balance and portfolio.

This creates additional correctness requirements:

* Orders across symbols must follow one deterministic global sequence.
* Cross-sectional signals must use a consistent observation cutoff.
* Missing or delayed bars must not accidentally introduce future information.
* Portfolio cash and buying-power constraints must be applied identically.
* Multi-leg orders need explicitly defined atomicity or partial-execution semantics.

For the course project, one Raft group controlling the entire portfolio is preferable to separate Raft groups per symbol. Separate groups introduce cross-shard coordination and atomicity problems that are outside your core research question.

Alpaca supports multiple symbols in one historical-bars request. Its multi-symbol response is ordered by symbol first, then timestamp, and pagination limits apply across the entire response rather than separately to each ticker. You must collect all pages and merge the symbols into a deterministic chronological sequence.&#x20; 

[image](https://www.google.com/s2/favicons?domain=https://docs.alpaca.markets\&sz=32)

docs.alpaca.markets

## 9. How API selection affects Raft itself

This is the architectural decision that matters most.

There are two ways to structure consensus regardless of which Alpaca APIs you choose.

Architecture 1 — Replicate orders only

Historical data → Shared deterministic replay

Strategy → Raft order log

Replica execution engines

Lower consensus load, but market-data synchronization must be guaranteed separately. Every order must reference a precise market-state version.

Architecture 2 — Replicate market events and orders

Historical data → Unified event stream

Raft log: MarketEvent + SubmitOrder + CancelOrder

Identical market, order, fill and portfolio state

Stronger self-contained auditability and a more demanding consensus workload, but larger logs and greater throughput requirements.

| Consideration                | Orders-only Raft            | Full-event Raft                    |
| ---------------------------- | --------------------------- | ---------------------------------- |
| Log size                     | Small                       | Potentially very large             |
| Consensus throughput         | Driven by order count       | Driven by market events and orders |
| Data synchronization         | External responsibility     | Inside consensus                   |
| Crash recovery               | Restore log + correct trace | Restore replicated state/log       |
| Auditability                 | Requires trace manifest     | More self-contained                |
| Implementation effort        | Lower                       | Higher                             |
| Distributed systems research | Strong                      | Very strong                        |

An important nuance: Raft guarantees agreement on the committed log, not automatically identical application state. You must still ensure deterministic arithmetic, command ordering, input data, and state-machine behavior.

For a full-event design, I would replicate the market-event stream and order commands but derive fills inside the state machine. Replicating independently generated fills as additional commands can create unnecessary complexity and duplicate-execution risks.

## 10. How much data would each configuration generate?

The following figures illustrate potential replay workload sizes. They are not measured Alpaca record counts.

### Historical replay workload estimator

Data configuration

OHLCV only

OHLCV + Quotes

OHLCV + Trades

OHLCV + Trades + Quotes

Trades + Quotes

Trading days

20

Symbols

1

Estimated market events

# 307, 800

Assumptions: 390 one-minute bars, 5, 000 trades and 10, 000 quote updates per symbol per day. These are illustrative workload parameters, not SPY feed measurements.

Full-event Raft entries

## 307, 800

Excludes strategy orders

Orders-only Raft entries

## Variable

Depends on strategy activity

For example, a 20-day SPY-only replay using one-minute bars gives approximately 7, 800 regular-session bar slots. A three-symbol portfolio increases that to approximately 23, 400 slots, before accounting for missing bars or different trading calendars.

Quotes and trades can increase event counts dramatically, but the actual counts depend on feed, symbol, market conditions, and filtering.

## 11. Recommended experimental design

Rather than commit to one API combination permanently, I would structure the project as a controlled experiment with interchangeable data adapters.

| Experiment | Data            | Symbols       | Raft architecture | Research purpose                             |
| ---------- | --------------- | ------------- | ----------------- | -------------------------------------------- |
| E1         | OHLCV           | SPY           | Orders only       | Establish correctness baseline               |
| E2         | OHLCV           | SPY           | Full event        | Measure replication overhead                 |
| E3         | OHLCV + Quotes  | SPY           | Full event        | Evaluate spread-aware execution              |
| E4         | Trades + Quotes | SPY           | Full event        | Stress higher-frequency replay               |
| E5         | OHLCV           | SPY, QQQ, IWM | Full event        | Validate multi-asset portfolio consistency   |
| E6         | OHLCV + Quotes  | SPY, QQQ, IWM | Full event        | Cross-sectional execution and fault recovery |

Keep the consensus implementation, hardware, fault-injection schedule, and accounting logic consistent between experiments. Otherwise it becomes difficult to attribute performance differences to the market-data configuration.

Also separate replay event count from committed order count. A configuration generating more quote updates should not automatically be interpreted as a more demanding order-execution workload.

## 12. Suggested Rust project structure

```
raft-execution-sandbox/
├── crates/
│   ├── alpaca-data/
│   │   ├── bars.rs
│   │   ├── trades.rs
│   │   ├── quotes.rs
│   │   └── pagination.rs
│   ├── market-replay/
│   │   ├── events.rs
│   │   ├── scheduler.rs
│   │   └── trace.rs
│   ├── strategies/
│   │   ├── momentum.rs
│   │   ├── mean_reversion.rs
│   │   └── cross_sectional.rs
│   ├── matching-engine/
│   │   ├── orders.rs
│   │   ├── fills.rs
│   │   ├── execution.rs
│   │   └── portfolio.rs
│   ├── raft-node/
│   │   ├── consensus.rs
│   │   ├── storage.rs
│   │   └── state_machine.rs
│   └── experiments/
│       ├── fault_injection.rs
│       ├── metrics.rs
│       └── validation.rs
├── configs/
│   ├── ohlcv_only.yaml
│   ├── ohlcv_quotes.yaml
│   ├── trades_quotes.yaml
│   └── multi_symbol.yaml
├── data/
│   └── historical/
└── results/
```

The advantage of this structure is that adding a new API or a cross-sectional strategy doesn't require rewriting the Raft implementation.

## 13. Final recommendation

Best initial configuration

OHLCV + Quotes

Use SPY 1-minute bars to generate strategy signals and historical quotes to model execution. Start with a deterministic single-node simulator, then add a three-node Raft cluster.

Best consensus experiment

Compare orders-only replication against full-event replication, including leader crashes, retries, and recovery.

Best scalability extension

Add trades and quotes to create a much larger event-driven replay workload.

Best portfolio extension

Add SPY, QQQ, and IWM using synchronized OHLCV bars and a shared replicated portfolio.

One practical detail: Alpaca's current documentation lists historical equities data back to 2016, with the free Basic plan limited to 200 historical requests per minute and a 15-minute recent-data restriction. Its IEX feed is a single-exchange view, whereas SIP provides consolidated market coverage. Keep the data feed fixed within each experiment to ensure comparable results.&#x20; 

[image](https://www.google.com/s2/favicons?domain=https://docs.alpaca.markets\&sz=32)

docs.alpaca.markets

+1

The most compelling final project would demonstrate that the same Raft cluster maintains identical committed order, fill, and portfolio state under three different workloads: bar-driven SPY trading, quote-aware SPY execution, and multi-symbol cross-sectional trading. That provides a clear progression from correctness to market-data complexity to distributed-system performance, without making the project dependent on the profitability of any particular trading strategy.
