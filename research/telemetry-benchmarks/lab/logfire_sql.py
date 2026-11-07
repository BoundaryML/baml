"""Run a Logfire SQL query with the test-project token from ../.env."""
import sys
from pathlib import Path

import requests

S = dict(l.strip().split("=", 1) for l in open(Path(__file__).resolve().parents[1] / ".env") if "=" in l)


def query(sql):
    r = requests.get("https://logfire-us.pydantic.dev/v1/query", timeout=120, params={"sql": sql},
                     headers={"Authorization": "Bearer " + S["LOGFIRE_TOKEN"], "Accept": "application/json"})
    if r.status_code >= 400:
        raise RuntimeError(f"{r.status_code}: {r.text[:500]}")
    cols = r.json()["columns"]
    return [dict(zip([c["name"] for c in cols], row)) for row in zip(*[c["values"] for c in cols])]


if __name__ == "__main__":
    for row in query(sys.argv[1]):
        print(row)
