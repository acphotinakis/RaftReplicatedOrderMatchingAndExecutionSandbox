from __future__ import annotations

import argparse
import csv
import json
import logging
import os
import shutil
import time
from datetime import date, datetime, time as dt_time, timedelta, timezone
from pathlib import Path
from typing import Any, Iterator

import requests
from dotenv import load_dotenv

from config import DEFAULT_SYMBOLS
from partition_market_data import partition_csv_by_date
from rate_limiter import RateLimiter

BASE_URL = "https://data.alpaca.markets/v2/stocks/trades"
MAX_LIMIT = 10_000

CSV_FIELDS = [
    "timestamp",
    "symbol",
    "price",
    "size",
    "exchange",
    "trade_id",
    "conditions",
    "update",
    "tape",
]

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s | %(levelname)s | %(message)s",
)
logger = logging.getLogger("alpaca-history")


def five_years_ago(value: datetime) -> datetime:
    """Subtract five calendar years, handling Feb. 29 safely."""
    try:
        return value.replace(year=value.year - 5)
    except ValueError:
        # Feb. 29 -> Feb. 28 in the target year.
        return value.replace(year=value.year - 5, day=28)


def parse_iso_datetime(value: str) -> datetime:
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def rfc3339(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def end_of_day_rfc3339(day: date) -> str:
    # Alpaca accepts RFC-3339 timestamps with nanosecond precision. Using the
    # last nanosecond of the day prevents overlap with the next day's window.
    return f"{day.isoformat()}T23:59:59.999999999Z"


def iter_daily_windows(
    start: datetime, end: datetime
) -> Iterator[tuple[str, str, date]]:
    """
    Yield non-overlapping UTC day windows.

    The first and last windows preserve the requested timestamps; complete
    intermediate days use midnight through 23:59:59.999999999.
    """
    current_day = start.date()

    while current_day <= end.date():
        if current_day == start.date():
            window_start = rfc3339(start)
        else:
            window_start = f"{current_day.isoformat()}T00:00:00Z"

        if current_day == end.date():
            window_end = rfc3339(end)
        else:
            window_end = end_of_day_rfc3339(current_day)

        yield window_start, window_end, current_day
        current_day += timedelta(days=1)


class HistoricalTradeDownloader:
    def __init__(
        self,
        api_key: str,
        secret_key: str,
        output_dir: Path,
        parquet_output_dir: Path | None = None,
        feed: str = "sip",
        limit: int = MAX_LIMIT,
        timeout: int = 60,
        max_retries: int = 8,
        requests_per_minute: int = 150,
    ) -> None:
        self.output_dir = output_dir
        self.parquet_output_dir = parquet_output_dir
        self.feed = feed
        self.limit = min(max(1, limit), MAX_LIMIT)
        self.timeout = timeout
        self.max_retries = max_retries

        self.output_dir.mkdir(parents=True, exist_ok=True)
        if self.parquet_output_dir is not None:
            self.parquet_output_dir.mkdir(parents=True, exist_ok=True)
        self.tmp_dir = self.output_dir / ".historical_tmp"
        self.tmp_dir.mkdir(parents=True, exist_ok=True)
        self.state_dir = self.output_dir / ".historical_state"
        self.state_dir.mkdir(parents=True, exist_ok=True)

        self.session = requests.Session()
        self.session.headers.update(
            {
                "accept": "application/json",
                "APCA-API-KEY-ID": api_key,
                "APCA-API-SECRET-KEY": secret_key,
            }
        )

        self.rate_limiter = RateLimiter(
            requests_per_minute=requests_per_minute,
        )

    def _request_page(self, params: dict[str, Any]) -> dict[str, Any]:
        delay = 1.0

        for attempt in range(1, self.max_retries + 1):
            try:
                self.rate_limiter.wait()

                response = self.session.get(
                    BASE_URL,
                    params=params,
                    timeout=self.timeout,
                )

                remaining = response.headers.get("X-Ratelimit-Remaining")
                reset = response.headers.get("X-Ratelimit-Reset")
                limit = response.headers.get("X-Ratelimit-Limit")

                logger.debug(
                    "Rate limit | limit=%s remaining=%s reset=%s",
                    limit,
                    remaining,
                    reset,
                )

                if response.status_code == 429:
                    retry_after = response.headers.get("Retry-After")
                    sleep_for = float(retry_after) if retry_after else delay
                    logger.warning(
                        "Rate limited (429). Sleeping %.1fs before retry %d/%d.",
                        sleep_for,
                        attempt,
                        self.max_retries,
                    )
                    time.sleep(sleep_for)
                    delay = min(delay * 2, 60)
                    continue

                if 500 <= response.status_code < 600:
                    logger.warning(
                        "Alpaca server error %s. Sleeping %.1fs before retry %d/%d.",
                        response.status_code,
                        delay,
                        attempt,
                        self.max_retries,
                    )
                    time.sleep(delay)
                    delay = min(delay * 2, 60)
                    continue

                response.raise_for_status()
                return response.json()

            except requests.RequestException as exc:
                if attempt == self.max_retries:
                    raise
                logger.warning(
                    "Request failed: %s. Sleeping %.1fs before retry %d/%d.",
                    exc,
                    delay,
                    attempt,
                    self.max_retries,
                )
                time.sleep(delay)
                delay = min(delay * 2, 60)

        raise RuntimeError("Historical trade request exhausted retries.")

    @staticmethod
    def _trade_to_row(symbol: str, trade: dict[str, Any]) -> dict[str, Any]:
        conditions = trade.get("c") or []
        if isinstance(conditions, list):
            conditions = "|".join(str(item) for item in conditions)

        return {
            "timestamp": trade.get("t"),
            "symbol": symbol,
            "price": trade.get("p"),
            "size": trade.get("s"),
            "exchange": trade.get("x"),
            "trade_id": trade.get("i"),
            "conditions": conditions,
            "update": trade.get("u"),
            "tape": trade.get("z"),
        }

    def _completed_state_path(self, symbol: str) -> Path:
        return self.state_dir / f"{symbol}_{self.feed}_completed_days.json"

    def _load_completed_days(self, symbol: str) -> set[str]:
        path = self._completed_state_path(symbol)
        if not path.exists():
            return set()

        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
            return set(payload.get("completed_days", []))
        except (json.JSONDecodeError, OSError):
            logger.warning(
                "Could not read state file %s; starting with empty state.", path
            )
            return set()

    def _save_completed_days(self, symbol: str, days: set[str]) -> None:
        path = self._completed_state_path(symbol)
        tmp = path.with_suffix(".tmp")
        tmp.write_text(
            json.dumps(
                {
                    "symbol": symbol,
                    "feed": self.feed,
                    "completed_days": sorted(days),
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        tmp.replace(path)

    def _main_csv(self, symbol: str) -> Path:
        return self.output_dir / f"{symbol}_historical_trades_{self.feed}.csv"

    def _part_csv(self, symbol: str, day: date) -> Path:
        return self.tmp_dir / f"{symbol}_{day.isoformat()}_{self.feed}.part.csv"

    def _append_part_to_main(self, part_path: Path, symbol: str) -> None:
        main_path = self._main_csv(symbol)
        main_exists = main_path.exists() and main_path.stat().st_size > 0

        with main_path.open("a", newline="", encoding="utf-8") as dst:
            with part_path.open("r", newline="", encoding="utf-8") as src:
                if main_exists:
                    # Skip the part file's header.
                    next(src, None)
                shutil.copyfileobj(src, dst)

    def download_day(
        self,
        symbol: str,
        window_start: str,
        window_end: str,
        day: date,
    ) -> int:
        part_path = self._part_csv(symbol, day)

        # A partial day is intentionally restarted from page 1. The completed
        # main CSV is only updated after the whole day finishes successfully.
        if part_path.exists():
            part_path.unlink()

        total = 0
        page_token: str | None = None

        with part_path.open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=CSV_FIELDS)
            writer.writeheader()

            while True:
                params: dict[str, Any] = {
                    "symbols": symbol,
                    "start": window_start,
                    "end": window_end,
                    "limit": self.limit,
                    "feed": self.feed,
                    "sort": "asc",
                }
                if page_token:
                    params["page_token"] = page_token

                payload = self._request_page(params)

                # Multi-symbol endpoint shape:
                # {"trades": {"AAPL": [...]}, "next_page_token": "..."}
                trades_by_symbol = payload.get("trades") or {}
                trades = trades_by_symbol.get(symbol, [])

                for trade in trades:
                    writer.writerow(self._trade_to_row(symbol, trade))

                total += len(trades)
                handle.flush()

                page_token = payload.get("next_page_token")
                if not page_token:
                    break

        if self.parquet_output_dir is None:
            self._append_part_to_main(part_path, symbol)
        else:
            partition_csv_by_date(
                part_path,
                self.parquet_output_dir,
                data_kind="trades",
                log_every_seconds=0,
                write_source_manifest=False,
            )
        part_path.unlink(missing_ok=True)
        return total

    def download_symbol(self, symbol: str, start: datetime, end: datetime) -> int:
        completed = self._load_completed_days(symbol)
        symbol_total = 0

        for window_start, window_end, day in iter_daily_windows(start, end):
            day_key = day.isoformat()

            if day_key in completed:
                logger.info("%s %s already completed; skipping.", symbol, day_key)
                continue

            logger.info(
                "%s | downloading %s | %s -> %s",
                symbol,
                day_key,
                window_start,
                window_end,
            )

            count = self.download_day(
                symbol=symbol,
                window_start=window_start,
                window_end=window_end,
                day=day,
            )
            symbol_total += count

            completed.add(day_key)
            self._save_completed_days(symbol, completed)

            logger.info("%s | %s | wrote %s trades", symbol, day_key, f"{count:,}")

        return symbol_total


def parse_args() -> argparse.Namespace:
    now = datetime.now(timezone.utc)
    default_start = five_years_ago(now)

    parser = argparse.ArgumentParser(
        description="Download Alpaca historical stock trades into one CSV per symbol."
    )
    parser.add_argument(
        "--symbols",
        default=os.getenv("ALPACA_SYMBOLS", ",".join(DEFAULT_SYMBOLS)),
        help="Comma-separated symbols. Defaults to ALPACA_SYMBOLS.",
    )
    parser.add_argument(
        "--start",
        default=rfc3339(default_start),
        help="RFC-3339 start timestamp. Default: exactly five calendar years ago.",
    )
    parser.add_argument(
        "--end",
        default=rfc3339(now),
        help="RFC-3339 end timestamp. Default: current UTC time.",
    )
    parser.add_argument(
        "--feed",
        default=os.getenv(
            "ALPACA_HISTORICAL_FEED",
            os.getenv("ALPACA_DATA_FEED", "sip"),
        ),
        choices=["sip", "iex", "boats", "otc"],
        help="Historical stock feed.",
    )
    parser.add_argument(
        "--output-dir",
        default=os.getenv("HISTORICAL_OUTPUT_DIR", "data/raw/trades"),
        help="Directory for historical CSV output.",
    )
    parser.add_argument(
        "--parquet-output-dir",
        default=None,
        help=(
            "Write atomic date-partitioned Parquet files here instead of "
            "appending to the monolithic CSV."
        ),
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=MAX_LIMIT,
        help="Trades requested per page (1-10000).",
    )
    parser.add_argument(
        "--requests-per-minute",
        type=int,
        default=int(
            os.getenv(
                "ALPACA_REQUESTS_PER_MINUTE",
                "150",
            )
        ),
        help=("Proactive API throttle. " "Default: 150 requests/minute."),
    )
    return parser.parse_args()


def main() -> None:
    load_dotenv()
    args = parse_args()

    api_key = os.getenv("ALPACA_API_KEY", "").strip()
    secret_key = os.getenv("ALPACA_SECRET_KEY", "").strip()
    if not api_key or not secret_key:
        raise RuntimeError(
            "Missing ALPACA_API_KEY / ALPACA_SECRET_KEY. "
            "Copy .env.example to .env and add your credentials."
        )

    symbols = [item.strip().upper() for item in args.symbols.split(",") if item.strip()]
    if not symbols:
        raise ValueError("At least one symbol is required.")
    if args.parquet_output_dir and len(symbols) != 1:
        raise ValueError(
            "--parquet-output-dir requires exactly one symbol per invocation"
        )

    start = parse_iso_datetime(args.start)
    end = parse_iso_datetime(args.end)
    if start >= end:
        raise ValueError("--start must be before --end.")

    downloader = HistoricalTradeDownloader(
        api_key=api_key,
        secret_key=secret_key,
        output_dir=Path(args.output_dir),
        parquet_output_dir=(
            Path(args.parquet_output_dir) if args.parquet_output_dir else None
        ),
        feed=args.feed,
        limit=args.limit,
        requests_per_minute=args.requests_per_minute,
    )

    logger.info("Historical trade backfill")
    logger.info("Symbols: %s", ",".join(symbols))
    logger.info("Feed: %s", args.feed)
    logger.info("Range: %s -> %s", rfc3339(start), rfc3339(end))
    logger.info("Output: %s", Path(args.output_dir).resolve())
    if args.parquet_output_dir:
        logger.info(
            "Parquet output: %s",
            Path(args.parquet_output_dir).resolve(),
        )

    grand_total = 0
    for symbol in symbols:
        count = downloader.download_symbol(symbol, start, end)
        grand_total += count
        logger.info(
            "%s finished this run with %s newly downloaded trades.",
            symbol,
            f"{count:,}",
        )

    logger.info(
        "Backfill complete. %s trades downloaded in this run.", f"{grand_total:,}"
    )


if __name__ == "__main__":
    main()
