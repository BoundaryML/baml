"""One-off: migrate baml-changelog2's MySQL entries into Convex changelogEntries.

Reads the old `entries` table and POSTs each row to the bench3 api as a
status=done changelogEntries row. Idempotent: versions already present are
skipped, so it can be re-run safely.

Usage:
    fly proxy 3306 -a baml-changelog2-mysql          # terminal 1
    DATABASE_URL='mysql://baml:<pw>@127.0.0.1:3306/changelog' \
    SERVICE_URL=https://bench3-api.fly.dev \
    ATB_SERVICE_TOKEN=... \
    uv run python scripts/migrate_changelog_mysql.py
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
from urllib.parse import urlparse

import pymysql  # dev-only dep: uv pip install pymysql

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "libs"))
from bench_core.service_client import ServiceClient  # noqa: E402


def read_mysql_entries() -> list[dict]:
    """Read every row of the old entries table.

    Returns:
        Row dicts with version/date/title/body/authors/channel/created_at.
    """
    url = urlparse(os.environ["DATABASE_URL"])
    conn = pymysql.connect(
        host=url.hostname or "127.0.0.1",
        port=url.port or 3306,
        user=url.username or "baml",
        password=url.password or "",
        database=(url.path or "/changelog").lstrip("/"),
        cursorclass=pymysql.cursors.DictCursor,
    )
    with conn, conn.cursor() as cur:
        cur.execute(
            "SELECT version, date, title, body, authors, channel, created_at "
            "FROM entries ORDER BY created_at ASC"
        )
        return list(cur.fetchall())


async def migrate() -> None:
    """Push each MySQL row into Convex, skipping versions already present."""
    rows = read_mysql_entries()
    print(f"mysql: {len(rows)} entries")
    service = ServiceClient(
        os.environ["SERVICE_URL"], os.environ.get("ATB_SERVICE_TOKEN", "")
    )
    created = skipped = 0
    try:
        existing = await service.list("changelogEntries", limit=2000)
        known = {e.get("version") for e in existing}
        for row in rows:
            version = row["version"]
            if version in known:
                skipped += 1
                continue
            authors = row.get("authors")
            if isinstance(authors, (str, bytes)):
                try:
                    authors = json.loads(authors)
                except json.JSONDecodeError:
                    authors = []
            await service.create("changelogEntries", {
                "version": version,
                "date": row.get("date"),
                "title": row.get("title"),
                "body": row.get("body"),
                "authors": authors or [],
                "channel": row.get("channel") or "unknown",
                "status": "done",
            })
            created += 1
            if created % 25 == 0:
                print(f"  ...{created} created")
    finally:
        await service.aclose()
    print(f"done: created={created} skipped(existing)={skipped}")


if __name__ == "__main__":
    asyncio.run(migrate())
