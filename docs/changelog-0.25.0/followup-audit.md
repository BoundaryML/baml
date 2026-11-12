# BAML 0.25.0 release audit

Prepared on 2026-11-05 Pacific time for the 2026-11-06 UTC release. All inventories, effects, examples, and follow-ups use these fixed boundaries.

- Lower: published `baml-language-0.24.0`, `dec4713a0e7c9699442bf5855b4f327063f769c5`.
- Upper: 0.25.0 release source, `a2f6d5403f399b0b3cc1a0902b0b3dd208c6043a`, selected by [release run 42150548387](https://github.com/BoundaryML/baml/actions/runs/42150548387). The tag was not yet created when these references were pinned. Verify its final target before publication.
- The range contains 14 squash-merged PRs across the whole repository. All are v1, including website, workflow, tests, and metadata PRs excluded from the public post.
- The 0.24.0 registry release did not promote the canary channel. #5551 explicitly replaces it with 0.25.0 instead of rerunning the old cut. The new post links the 0.24.0 migrations for users coming from the last promoted 0.23.0 canary.
- The existing changelog branch was rebased onto this upper source. The 0.24.0 notes and audits remain separate. This directory records the additional 0.25.0 inventory without overwriting the earlier audit.

## Source coverage

All 14 PR descriptions, 142 issue-comment/review bodies, 39 inline review comments, author associations, closing-issue references, and changed-file lists were read. All retrieved discussion and file connections reported no next page. Every PR was considered for notifications, including the eight excluded from the public release post.

Additional GitHub context was fetched for #5075, #5185, #5422, #5483, #5519, #5526, #5528, #5532, #5533, #5534, #5541, #5546, and #5548. Contextual predecessor PRs do not become new 0.25.0 contributions. BoundaryML/alien-baml-sdk#6 was read with its empty issue/inline comment lists because #5532 explicitly references it. It is authored internally and is not an external-user PR to announce.

[Linear BAMLGH-94](https://linear.app/boundaryml2/issue/BAMLGH-94) was read with both comments and its GitHub attachment. It mirrors #5075. No other Linear issue was directly referenced. TASK-121/TASK-122 were resolved in Notion rather than misidentified as Linear issues. Their source triage and customer-feedback pages were read, including four internal inline comments. [TASK-199](https://app.notion.com/p/40cbb2d2621681aa86beef18ed4a8b34) was read from the direct link in #5547. No discussion pagination was omitted.

The Alien task links led to the customer thread at `C0BSRAU8PPD`, timestamp `1793394616.813439`. The complete parent and replies were read through the connected Slack read-only tool. Itamar Zand and Alon Gubkin are the customer contacts. The actual reply asks for the shared-engine/Zod work, which is still open. Package-name isolation addresses only the separately recorded installation collision. This audit records only the delivered package-name portion; the shared-engine and Zod requests remain open.

No Discord thread link was found in the audited PRs, referenced GitHub sources, Linear mirror, or Notion/customer thread. There is no invented Discord recipient. Credentials and raw private evidence are not committed.

## Decisions

| Source | Decision | Scope |
| --- | --- | --- |
| [#5501](https://github.com/BoundaryML/baml/pull/5501) | External contributor: @Nedas-Jaronis. | Merged removal of unreachable stdlib fallbacks and regression coverage. |
| [#5075](https://github.com/BoundaryML/baml/issues/5075) | External reporter: @BenSpex. | Unactionable stdlib warnings fixed. Do not imply unreachable diagnostics for user code were disabled. |
| Alien package-isolation request | Customer context for package isolation. | Configurable package names fix install collisions. One program/type map per process and exact bridge-version requirements remain. No Slack message was sent. |
| TASK-121 / shared engine and Zod | Do not announce as fixed. | They remain separate work, even though the package-name part ships. |
| Remaining in-range authors | No external-author PR notification. | All are MEMBER; only #5501 has CONTRIBUTOR association. |

The per-release action checklists were removed. This file retains the source and attribution audit; no notifications were sent.
