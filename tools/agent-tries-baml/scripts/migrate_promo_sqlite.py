"""One-off: migrate the t-shirts bot's SQLite codes into Convex promoCodes.

Reads the old `codes` table (promo.db) and POSTs each row to the bench3 api,
preserving code/position/status/claimed_by/notes/claimed_at. Idempotent: codes
already present are skipped, so it can be re-run safely.

Usage:
    fly ssh sftp get /data/promo.db ./promo.db -a promobot   # grab the db
    PROMO_DB=./promo.db \
    SERVICE_URL=https://bench3-api.fly.dev \
    ATB_SERVICE_TOKEN=... \
    uv run python scripts/migrate_promo_sqlite.py
"""

from __future__ import annotations

import asyncio
import os
import sqlite3
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "libs"))
from bench_core.service_client import ServiceClient  # noqa: E402


def read_codes() -> list[dict]:
    """Read every row of the old codes table.

    Returns:
        Row dicts with code/position/status/claimed_by/claimed_by_user_id/
        notes/claimed_at.
    """
    conn = sqlite3.connect(os.environ.get("PROMO_DB", "./promo.db"))
    conn.row_factory = sqlite3.Row
    with conn:
        rows = conn.execute(
            "SELECT code, position, status, claimed_by, claimed_by_user_id, "
            "notes, claimed_at FROM codes ORDER BY position ASC"
        ).fetchall()
    return [dict(r) for r in rows]


def _epoch_ms(value) -> int | None:
    """Best-effort conversion of the old claimed_at to epoch millis.

    Args:
        value: An ISO string, epoch seconds, epoch millis, or None.

    Returns:
        Epoch milliseconds, or None when unparsable/absent.
    """
    if value is None:
        return None
    if isinstance(value, (int, float)):
        return int(value if value > 1e12 else value * 1000)
    from datetime import datetime
    try:
        return int(datetime.fromisoformat(str(value)).timestamp() * 1000)
    except ValueError:
        return None


async def migrate() -> None:
    """Push each SQLite row into Convex, skipping codes already present."""
    rows = read_codes()
    print(f"sqlite: {len(rows)} codes "
          f"({sum(1 for r in rows if r['status'] == 'unused')} unused)")
    service = ServiceClient(
        os.environ["SERVICE_URL"], os.environ.get("ATB_SERVICE_TOKEN", "")
    )
    created = skipped = 0
    try:
        existing = await service.list("promoCodes", limit=2000)
        known = {e.get("code") for e in existing}
        for row in rows:
            if row["code"] in known:
                skipped += 1
                continue
            await service.create("promoCodes", {
                "code": row["code"],
                "position": row["position"],
                "status": row["status"],
                "claimedBy": row.get("claimed_by"),
                "claimedByUserId": row.get("claimed_by_user_id"),
                "notes": row.get("notes"),
                "claimedAt": _epoch_ms(row.get("claimed_at")),
            })
            created += 1
            if created % 50 == 0:
                print(f"  ...{created} created")
    finally:
        await service.aclose()
    print(f"done: created={created} skipped(existing)={skipped}")


if __name__ == "__main__":
    asyncio.run(migrate())
