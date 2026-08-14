"""Integration driver: create a task, run BamlWorker once (it assembles the whole
trophy) then BamlDedup, against the live service/Convex + a stub proxy, and assert.
Run with SERVICE_URL + CLAUDE_PROXY_URLS env pointing at running stubs.
"""

import asyncio
import os

from bench_core.service_client import ServiceClient
from services.baml_worker.__main__ import BamlWorker
from services.baml_dedup.__main__ import BamlDedup


async def main() -> None:
    """Drive the full task-to-issue pipeline once and assert each stage.

    Creates a task, runs BamlWorker (assembles the trophy), BamlDedup (creates the
    issue), and NotionPush (no-creds sync path), asserting the trophy outcome,
    findings/suggestions, the deduped issue, and its Notion sync state along the way.

    Raises:
        AssertionError: If any stage produces an unexpected result.
    """
    service = ServiceClient(os.environ["SERVICE_URL"], os.environ.get("SERVICE_TOKEN", ""))
    try:
        task_id = await service.create("tasks", {
            "source": "bug_report",
            "prompt": "implement an enum with an aliased value in baml",
            "status": "queued",
        })
        print("created task", task_id)

        worker = BamlWorker(service)
        await worker._drain()
        t = await service.get("tasks", task_id)
        assert t["status"] == "done", f"expected done, got {t['status']}"
        print("worker OK: task -> done (trophy assembled)")

        trophies = await service.list("trophies", field="status", value="queued",
                                  index="by_status_created")
        mine = [x for x in trophies if x["taskId"] == task_id]
        assert mine, "no trophy created"
        tr = mine[0]
        # outcome reflects task completion (success), independent of findings
        assert tr["outcome"] == "success", tr["outcome"]
        assert tr["metrics"]["api_calls"] == 2
        assert tr.get("reportMd"), "trophy should carry the agent's report_md"
        assert tr["findings"] and tr["findings"][0]["kind"] == "language"
        assert tr["findings"][0]["anchor"]["call_index"] == 2
        assert tr["findings"][0].get("suggestion"), "finding should carry a suggestion"
        assert tr.get("suggestions"), "trophy should carry top-level suggestions"
        print(f"worker OK: trophy outcome={tr['outcome']} "
              f"findings={len(tr['findings'])} suggestions={len(tr['suggestions'])}")

        trophy_id = tr["_id"]
        baml_dedup = BamlDedup(service)
        await baml_dedup._drain()
        tr2 = await service.get("trophies", trophy_id)
        assert tr2["status"] == "done", f"trophy should be deduped/done, got {tr2['status']}"
        issues = await service.list("issues", limit=50)
        mine = [i for i in issues if any(e.get("trophyId") == trophy_id
                                         for e in (i.get("evidence") or []))]
        assert mine, "no issue created referencing this trophy"
        iss = mine[0]
        assert iss["kind"] == "language" and iss["status"] == "open"
        assert iss["notionSyncStatus"] == "dirty"
        assert iss.get("suggestion"), "issue should carry a suggestion"
        assert iss.get("category") == "bug", iss.get("category")
        ev = iss["evidence"][0]
        assert ev["trophyId"] == trophy_id and ev["call_index"] == 2
        print(f"baml-dedup OK: issue kind={iss['kind']} status={iss['status']} "
              f"evidence->trophy={ev['trophyId']==trophy_id} call={ev['call_index']}")

        from services.notion_fixer.__main__ import NotionPush
        push = NotionPush(service)
        await push._drain()
        iss2 = await service.get("issues", iss["_id"])
        assert iss2["notionSyncStatus"] == "synced", iss2["notionSyncStatus"]
        assert iss2["status"] == "confirmed", iss2["status"]
        print(f"notion-push OK (no-creds path): issue -> status={iss2['status']} "
              f"sync={iss2['notionSyncStatus']}")
        print("PIPELINE_OK")
    finally:
        await service.aclose()


if __name__ == "__main__":
    asyncio.run(main())
