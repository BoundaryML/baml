# External-user followup research for BAML 0.19.0

Research date: September 11, 2026, America/Los_Angeles. Scope: the [pinned 97-commit inventory](step1a-full-commit-inventory.md). All 97 PR records were fetched; the required v1 audit includes 89 PRs, excluding only eight v0-only PRs. Internal changes and external contributions to development documentation remain in this audit.

## Method and identity checks

Read PR descriptions, review comments, reviews, issue comments, commit contributors, linked issues, and local merged diffs. REST review comments, reviews, commits, and issue comments were paginated. The 97 PR issue-comment inventories contain 501 comments. Followed GitHub/Linear issue references through their comments and attachments to a fixed point. Cross-referenced historical PRs to distinguish already-shipped fixes from this release. Review-generated historical examples in #4784 are evidence to investigate, not automatic notification candidates.

The authenticated BoundaryML organization roster identifies `2kai2kai2`, `aaronvg`, `anish-palakurthi`, `antoniosarosi`, `ATX24`, `codeshaunted`, `hellovai`, `rossirpaulo`, and `sxlijin` as internal. Of the human authors, commit contributors, and discussion participants on the 97 PRs, only `ritunjaym` is external, on #4751 and #4752. Bot and AI attribution is not treated as an external-user notification target. External issue reporters are identified separately below.

Linear credentials and the Discord ops-bot token were read from the instructed Infisical project/environment without committing credentials or raw private records. Linear comments were paginated; no fetched issue reports more comment pages. The two unavailable legacy Linear IDs are documented below.

Discord REST authenticated successfully and returned channel/thread/author metadata, but message bodies were empty. Used the already signed-in browser to read the relevant conversations, including the #4751 proposal and followup, the historical #1724 link, and a documentation-access candidate. No bot permissions were changed. No messages were sent.

## Every v1 PR

“No target” means no external author/contributor/report or newly fixed external issue was identified for that PR. Linked historical issues still appear in the issue dispositions below. The full inventory holds the separate user-visible/excluded changelog decision.

| PR | Author | Referenced issue evidence | Notification disposition |
| --- | --- | --- | --- |
| [#4625](https://github.com/BoundaryML/baml/pull/4625) | `codeshaunted` | No report references found. | No target identified. |
| [#4630](https://github.com/BoundaryML/baml/pull/4630) | `2kai2kai2` | No report references found. | No target identified. |
| [#4633](https://github.com/BoundaryML/baml/pull/4633) | `antoniosarosi` | No report references found. | No target identified. |
| [#4623](https://github.com/BoundaryML/baml/pull/4623) | `aaronvg` | Followed #4370 via linked Linear history; boundary generator comparison confirms fix | Draft scoped #4370 update; #4371 still reproduces. |
| [#4616](https://github.com/BoundaryML/baml/pull/4616) | `sxlijin` | No report references found. | No target identified. |
| [#4644](https://github.com/BoundaryML/baml/pull/4644) | `sxlijin` | No report references found. | No target identified. |
| [#4646](https://github.com/BoundaryML/baml/pull/4646) | `hellovai` | No report references found. | No target identified. |
| [#4632](https://github.com/BoundaryML/baml/pull/4632) | `sxlijin` | [#4624](https://github.com/BoundaryML/baml/issues/4624) | Gate the public changelog claim and #4624 update on a new published wrapper and bootstrap verification. |
| [#4680](https://github.com/BoundaryML/baml/pull/4680) | `sxlijin` | No report references found. | No target identified. |
| [#4687](https://github.com/BoundaryML/baml/pull/4687) | `sxlijin` | No report references found. | No target identified. |
| [#4686](https://github.com/BoundaryML/baml/pull/4686) | `2kai2kai2` | No report references found. | No target identified. |
| [#4684](https://github.com/BoundaryML/baml/pull/4684) | `sxlijin` | No report references found. | No target identified. |
| [#4688](https://github.com/BoundaryML/baml/pull/4688) | `sxlijin` | `B-1672` | No target identified. |
| [#4692](https://github.com/BoundaryML/baml/pull/4692) | `sxlijin` | `B-1656`; #1259 via B-1656 → BAMLGH-4 | Additional v1-specific update for external participants on maintainer-opened #1259. |
| [#4693](https://github.com/BoundaryML/baml/pull/4693) | `sxlijin` | `B-1672` | No target identified. |
| [#4698](https://github.com/BoundaryML/baml/pull/4698) | `sxlijin` | No report references found. | No target identified. |
| [#4685](https://github.com/BoundaryML/baml/pull/4685) | `sxlijin` | No report references found. | No target identified. |
| [#4701](https://github.com/BoundaryML/baml/pull/4701) | `sxlijin` | No report references found. | No target identified. |
| [#4714](https://github.com/BoundaryML/baml/pull/4714) | `antoniosarosi` | No report references found. | No target identified. |
| [#4713](https://github.com/BoundaryML/baml/pull/4713) | `antoniosarosi` | `B-1672`; `B-1675` | No target identified. |
| [#4718](https://github.com/BoundaryML/baml/pull/4718) | `sxlijin` | No report references found. | No target identified. |
| [#4611](https://github.com/BoundaryML/baml/pull/4611) | `codeshaunted` | `B-1646` | No target identified. |
| [#4702](https://github.com/BoundaryML/baml/pull/4702) | `sxlijin` | No report references found. | No target identified. |
| [#4703](https://github.com/BoundaryML/baml/pull/4703) | `sxlijin` | No report references found. | No target identified. |
| [#4704](https://github.com/BoundaryML/baml/pull/4704) | `sxlijin` | No report references found. | No target identified. |
| [#4705](https://github.com/BoundaryML/baml/pull/4705) | `sxlijin` | No report references found. | No target identified. |
| [#4706](https://github.com/BoundaryML/baml/pull/4706) | `sxlijin` | No report references found. | No target identified. |
| [#4707](https://github.com/BoundaryML/baml/pull/4707) | `sxlijin` | No report references found. | No target identified. |
| [#4708](https://github.com/BoundaryML/baml/pull/4708) | `sxlijin` | No report references found. | No target identified. |
| [#4709](https://github.com/BoundaryML/baml/pull/4709) | `sxlijin` | No report references found. | No target identified. |
| [#4634](https://github.com/BoundaryML/baml/pull/4634) | `ATX24` | [#4468](https://github.com/BoundaryML/baml/issues/4468); [#4587](https://github.com/BoundaryML/baml/issues/4587) | No target identified. |
| [#4635](https://github.com/BoundaryML/baml/pull/4635) | `ATX24` | No report references found. | No target identified. |
| [#4636](https://github.com/BoundaryML/baml/pull/4636) | `ATX24` | No report references found. | No target identified. |
| [#4637](https://github.com/BoundaryML/baml/pull/4637) | `ATX24` | No report references found. | No target identified. |
| [#4720](https://github.com/BoundaryML/baml/pull/4720) | `2kai2kai2` | `B-1680` | No target identified. |
| [#4723](https://github.com/BoundaryML/baml/pull/4723) | `codeshaunted` | `B-1610` | No target identified. |
| [#4725](https://github.com/BoundaryML/baml/pull/4725) | `2kai2kai2` | No report references found. | No target identified. |
| [#4639](https://github.com/BoundaryML/baml/pull/4639) | `ATX24` | No report references found. | No target identified. |
| [#4735](https://github.com/BoundaryML/baml/pull/4735) | `sxlijin` | No report references found. | No target identified. |
| [#4721](https://github.com/BoundaryML/baml/pull/4721) | `codeshaunted` | `B-1682` | No target identified. |
| [#4715](https://github.com/BoundaryML/baml/pull/4715) | `ATX24` | No report references found. | No target identified. |
| [#4738](https://github.com/BoundaryML/baml/pull/4738) | `sxlijin` | No report references found. | No target identified. |
| [#4711](https://github.com/BoundaryML/baml/pull/4711) | `sxlijin` | No report references found. | No target identified. |
| [#4739](https://github.com/BoundaryML/baml/pull/4739) | `2kai2kai2` | No report references found. | No target identified. |
| [#4727](https://github.com/BoundaryML/baml/pull/4727) | `hellovai` | No report references found. | No target identified. |
| [#4730](https://github.com/BoundaryML/baml/pull/4730) | `hellovai` | No report references found. | No target identified. |
| [#4732](https://github.com/BoundaryML/baml/pull/4732) | `hellovai` | No report references found. | No target identified. |
| [#4733](https://github.com/BoundaryML/baml/pull/4733) | `hellovai` | No report references found. | No target identified. |
| [#4734](https://github.com/BoundaryML/baml/pull/4734) | `hellovai` | No report references found. | No target identified. |
| [#4742](https://github.com/BoundaryML/baml/pull/4742) | `sxlijin` | No report references found. | No target identified. |
| [#4743](https://github.com/BoundaryML/baml/pull/4743) | `sxlijin` | No report references found. | No target identified. |
| [#4745](https://github.com/BoundaryML/baml/pull/4745) | `sxlijin` | No report references found. | No target identified. |
| [#4755](https://github.com/BoundaryML/baml/pull/4755) | `sxlijin` | No report references found. | No target identified. |
| [#4757](https://github.com/BoundaryML/baml/pull/4757) | `sxlijin` | No report references found. | No target identified. |
| [#4756](https://github.com/BoundaryML/baml/pull/4756) | `sxlijin` | [#2736](https://github.com/BoundaryML/baml/issues/2736) | No target identified. |
| [#4752](https://github.com/BoundaryML/baml/pull/4752) | `ritunjaym` | No report references found. | Draft contributor thanks despite end-user changelog exclusion. |
| [#4729](https://github.com/BoundaryML/baml/pull/4729) | `ATX24` | No report references found. | No target identified. |
| [#4759](https://github.com/BoundaryML/baml/pull/4759) | `antoniosarosi` | No report references found. | No target identified. |
| [#4762](https://github.com/BoundaryML/baml/pull/4762) | `hellovai` | No report references found. | No target identified. |
| [#4764](https://github.com/BoundaryML/baml/pull/4764) | `hellovai` | No report references found. | No target identified. |
| [#4777](https://github.com/BoundaryML/baml/pull/4777) | `codeshaunted` | No report references found. | No target identified. |
| [#4778](https://github.com/BoundaryML/baml/pull/4778) | `sxlijin` | No report references found. | No target identified. |
| [#4751](https://github.com/BoundaryML/baml/pull/4751) | `ritunjaym` | [#4750](https://github.com/BoundaryML/baml/issues/4750); Discord float proposal and #4750 | Draft PR thanks and dedicated Discord reply; keep #4750 open. |
| [#4779](https://github.com/BoundaryML/baml/pull/4779) | `sxlijin` | No report references found. | No target identified. |
| [#4780](https://github.com/BoundaryML/baml/pull/4780) | `sxlijin` | No report references found. | No target identified. |
| [#4783](https://github.com/BoundaryML/baml/pull/4783) | `hellovai` | No report references found. | No target identified. |
| [#4496](https://github.com/BoundaryML/baml/pull/4496) | `2kai2kai2` | No report references found. | No target identified. |
| [#4784](https://github.com/BoundaryML/baml/pull/4784) | `sxlijin` | [#1724](https://github.com/BoundaryML/baml/issues/1724); [#4154](https://github.com/BoundaryML/baml/issues/4154); [#4279](https://github.com/BoundaryML/baml/issues/4279); [#4335](https://github.com/BoundaryML/baml/issues/4335); [#4355](https://github.com/BoundaryML/baml/issues/4355); [#4371](https://github.com/BoundaryML/baml/issues/4371); [#4376](https://github.com/BoundaryML/baml/issues/4376); [#4421](https://github.com/BoundaryML/baml/issues/4421); [#4422](https://github.com/BoundaryML/baml/issues/4422); [#4429](https://github.com/BoundaryML/baml/issues/4429); [#4468](https://github.com/BoundaryML/baml/issues/4468); [#4497](https://github.com/BoundaryML/baml/issues/4497); [#4506](https://github.com/BoundaryML/baml/issues/4506); [#4588](https://github.com/BoundaryML/baml/issues/4588); [#4589](https://github.com/BoundaryML/baml/issues/4589) | No target identified. |
| [#4786](https://github.com/BoundaryML/baml/pull/4786) | `hellovai` | No report references found. | No target identified. |
| [#4794](https://github.com/BoundaryML/baml/pull/4794) | `sxlijin` | No report references found. | No target identified. |
| [#4785](https://github.com/BoundaryML/baml/pull/4785) | `sxlijin` | No report references found. | No target identified. |
| [#4791](https://github.com/BoundaryML/baml/pull/4791) | `sxlijin` | No report references found. | No target identified. |
| [#4798](https://github.com/BoundaryML/baml/pull/4798) | `sxlijin` | No report references found. | No target identified. |
| [#4788](https://github.com/BoundaryML/baml/pull/4788) | `sxlijin` | No report references found. | No target identified. |
| [#4781](https://github.com/BoundaryML/baml/pull/4781) | `codeshaunted` | `B-1676` | No target identified. |
| [#4803](https://github.com/BoundaryML/baml/pull/4803) | `sxlijin` | No report references found. | No target identified. |
| [#4736](https://github.com/BoundaryML/baml/pull/4736) | `sxlijin` | No report references found. | No target identified. |
| [#4800](https://github.com/BoundaryML/baml/pull/4800) | `codeshaunted` | No report references found. | No target identified. |
| [#4804](https://github.com/BoundaryML/baml/pull/4804) | `2kai2kai2` | No report references found. | No target identified. |
| [#4799](https://github.com/BoundaryML/baml/pull/4799) | `codeshaunted` | No report references found. | No target identified. |
| [#4819](https://github.com/BoundaryML/baml/pull/4819) | `sxlijin` | No report references found. | No target identified. |
| [#4820](https://github.com/BoundaryML/baml/pull/4820) | `codeshaunted` | `B-1470` | No target identified. |
| [#4806](https://github.com/BoundaryML/baml/pull/4806) | `aaronvg` | No report references found. | No target identified. |
| [#4816](https://github.com/BoundaryML/baml/pull/4816) | `hellovai` | No report references found. | No target identified. |
| [#4805](https://github.com/BoundaryML/baml/pull/4805) | `hellovai` | No report references found. | No target identified. |
| [#4807](https://github.com/BoundaryML/baml/pull/4807) | `aaronvg` | No report references found. | No target identified. |
| [#4836](https://github.com/BoundaryML/baml/pull/4836) | `hellovai` | No report references found. | No target identified. |
| [#4834](https://github.com/BoundaryML/baml/pull/4834) | `2kai2kai2` | No report references found. | No target identified. |
| [#4808](https://github.com/BoundaryML/baml/pull/4808) | `aaronvg` | No report references found. | No target identified. |

The v0-only PRs omitted from this table are #4642, #4643, #4645, #4659, #4663, #4669, #4670, and #4710. #4711 is mixed/shared development setup and was audited as v1.

## GitHub issue dispositions

Closed status alone is not proof of a fix in this range. Notifications require a matching merged change and a net improvement over the lower boundary.

| Issue | Reporter | Decision and evidence |
| --- | --- | --- |
| [#1259](https://github.com/BoundaryML/baml/issues/1259) | `aaronvg` | Additional scoped followup: #4692 repairs v1 workerd initialization. The issue is maintainer-opened; Udbhav8 and aretrace are external participants. Do not promise classic native Node SDK support. |
| [#1724](https://github.com/BoundaryML/baml/issues/1724) | `T9xz` | Historical v0 client<llm> transcription request linked from workflow examples; no new endpoint-support claim in this v1 release. Discord conversation read. |
| [#2736](https://github.com/BoundaryML/baml/issues/2736) | `Noir-Lime` | Historical v0 native Node glibc failure; distinct from the new language-wrapper bootstrap defect #4624. |
| [#4154](https://github.com/BoundaryML/baml/issues/4154) | `conrad-scherb` | v0 prompt-cache fix; maintainer already announced 0.224.0. Outside this language release. |
| [#4279](https://github.com/BoundaryML/baml/issues/4279) | `w0r1dhe110` | Prior musl pack/ELF fix; not the wrapper bootstrap change. No new-fix notification for #4632. |
| [#4335](https://github.com/BoundaryML/baml/issues/4335) | `BenSpex` | Prior Rust naming-convention diagnostic issue; not the companion/API migration. No new-fix evidence in this range. |
| [#4355](https://github.com/BoundaryML/baml/issues/4355) | `tha-hammer` | Prior native Node musl package issue; distinct from installer wrapper selection. No new-fix notification. |
| [#4370](https://github.com/BoundaryML/baml/issues/4370) | `BenSpex` | Draft scoped fix notification. Lower generation skips a representable LLM function because of ai.errors.Failure; upper emits direct/spec/stream exports. Generation-only verification, not a provider call. Skipped-declaration warnings and #4371 remain. |
| [#4371](https://github.com/BoundaryML/baml/issues/4371) | `BenSpex` | No fixed notification. Reproduced the non-identifier string-literal union (graph.query/graph.diff) at both boundaries; both skip Route and Pick and exit 0. |
| [#4376](https://github.com/BoundaryML/baml/issues/4376) | `BenSpex` | Already addressed at the lower boundary: maintainer explicitly says 0.18.0 changes the default output directory. |
| [#4421](https://github.com/BoundaryML/baml/issues/4421) | `tha-hammer` | Historical loop/array reassignment regression, referenced through preparation examples; no new matching fix established in the pinned range. |
| [#4422](https://github.com/BoundaryML/baml/issues/4422) | `tha-hammer` | Historical multi-part documentation/DX report. The portal and matching skill improve documentation, but no complete or specific partial repro fix was established for these seven complaints. Do not present it as resolved. |
| [#4429](https://github.com/BoundaryML/baml/issues/4429) | `tha-hammer` | Open log-output complaint; no matching logging behavior change in the range. No notification. |
| [#4468](https://github.com/BoundaryML/baml/issues/4468) | `briancripe` | Prior Array.map inference abort; source/Linear history and maintainer response precede the lower boundary. No new notification. |
| [#4497](https://github.com/BoundaryML/baml/issues/4497) | `zeke-john` | v0 Fireworks response fix, already announced in 0.226.1. Excluded from language changelog. |
| [#4506](https://github.com/BoundaryML/baml/issues/4506) | `ritunjaym` | No new fix established. The exact supplied reproducer yields the same E0097 unnecessary-throws diagnostic at both boundaries; do not equate that with proving every unknown-method ICE path fixed. |
| [#4587](https://github.com/BoundaryML/baml/issues/4587) | `BenSpex` | Reporter confirmed fix on 0.17.1-nightly.20260824.b, before 0.18.0. No new notification. |
| [#4588](https://github.com/BoundaryML/baml/issues/4588) | `BenSpex` | Open CLI thrown-error rendering report; no complete or partial fix established against the pinned boundaries. No notification. |
| [#4589](https://github.com/BoundaryML/baml/issues/4589) | `BenSpex` | Prior optional nested SAP coercion report; distinct from #4807 journal media representation. No matching new coercion fix established. |
| [#4624](https://github.com/BoundaryML/baml/issues/4624) | `BenSpex` | Draft fix notification for #4632, held until independently versioned published wrapper/installer bootstrap is verified. The catalog advertises GNU/musl artifacts but the wrapper tag predates the fix. |
| [#4641](https://github.com/BoundaryML/baml/issues/4641) | `vgatcg` | Concurrent client-cache panic fixed by v0-only #4643. Outside this v1 release. |
| [#4750](https://github.com/BoundaryML/baml/issues/4750) | `ritunjaym` | No fixed notification. Executing the issue function prints 0.0,0.0 at both boundaries. Explicitly left open in the float contribution replies. |

## Linear reference audit

Linear issue descriptions, comments, and attachments were checked for external origin and further report links. Internal tickets do not create public notification recipients. Synced GitHub issues are notified at GitHub rather than duplicated in Linear. Titles and internal incident details are deliberately not copied into this public review artifact.

| Linear issue | Origin / disposition |
| --- | --- |
| [B-1071](https://linear.app/boundaryml2/issue/B-1071/logical-operators-and-conditions-never-require-bool) | Internal source/context; no external report recipient identified. |
| [B-1073](https://linear.app/boundaryml2/issue/B-1073/literal-patterns-compare-with-which-coerces-across-the-numeric-tower) | Internal source/context; no external report recipient identified. |
| [B-1074](https://linear.app/boundaryml2/issue/B-1074/plain-field-access-on-a-nullable-intermediate-is-permitted-inside-an) | Internal source/context; no external report recipient identified. |
| [B-1123](https://linear.app/boundaryml2/issue/B-1123/bytecode-version-skew-errors) | Internal source/context; no external report recipient identified. |
| [B-1165](https://linear.app/boundaryml2/issue/B-1165/improve-demo-failure-errors-from-baml-bytecode-deserialization) | Internal source/context; no external report recipient identified. |
| [B-1180](https://linear.app/boundaryml2/issue/B-1180/vm-internal-error-method-call-on-an-interface-typed-optional) | Internal source/context; no external report recipient identified. |
| [B-1181](https://linear.app/boundaryml2/issue/B-1181/with-an-empty-collection-literal-infers-t-instead-of-unifying) | Internal source/context; no external report recipient identified. |
| [B-1182](https://linear.app/boundaryml2/issue/B-1182/does-not-narrow-the-chain-every-later-link-must-be-even-for-non) | Internal source/context; no external report recipient identified. |
| [B-1470](https://linear.app/boundaryml2/issue/B-1470/compiler-panics-on-interface-method-calls-inside-closures) | Internal source/context; no external report recipient identified. |
| [B-1511](https://linear.app/boundaryml2/issue/B-1511/bep-020-ruling-needed-does-guard-the-whole-chain-or-only-its-immediate) | Internal source/context; no external report recipient identified. |
| [B-1512](https://linear.app/boundaryml2/issue/B-1512/vm-internal-error-should-never-reach-users-audit-all-sites-and-panic) | Internal source/context; no external report recipient identified. |
| [B-1562](https://linear.app/boundaryml2/issue/B-1562/format-test-assertion-for-readability) | Internal source/context; no external report recipient identified. |
| [B-1563](https://linear.app/boundaryml2/issue/B-1563/allow-string-null-truthiness-in-if-conditions) | Internal source/context; no external report recipient identified. |
| [B-1576](https://linear.app/boundaryml2/issue/B-1576/fix-compiler-abort-for-inferred-arraymap-result) | GitHub origin/context: #4468. Apply the issue disposition above. |
| [B-1580](https://linear.app/boundaryml2/issue/B-1580/infer-callback-throws-effects-through-optional-callback-parameters) | Internal source/context; no external report recipient identified. |
| [B-1582](https://linear.app/boundaryml2/issue/B-1582/fix-remaining-runtime-reflection-and-dynamic-type-failures-found-by) | Maintainer-recorded external customer context. The linked #4501/#4519 fixes predate the lower boundary; no public report thread or named recipient was identified for a new notification. |
| [B-1588](https://linear.app/boundaryml2/issue/B-1588/sev-2-bug-baml-rejects-valid-fireworks-chat-completions-responses) | GitHub origin/context: #4497. Apply the issue disposition above. |
| [B-1591](https://linear.app/boundaryml2/issue/B-1591/standardize-c-sdk-generator-output-directory-on-baml-sdk) | Internal source/context; no external report recipient identified. |
| [B-1610](https://linear.app/boundaryml2/issue/B-1610/require-baml-skill-when-running-in-agent-mode) | Internal source/context; no external report recipient identified. |
| [B-1646](https://linear.app/boundaryml2/issue/B-1646/reject-unsafe-union-member-access-with-conflicting-types) | Internal source/context; no external report recipient identified. |
| [B-1649](https://linear.app/boundaryml2/issue/B-1649/optional-job-fields-return-null) | Internal source/context; no external report recipient identified. |
| [B-1655](https://linear.app/boundaryml2/issue/B-1655/open-question-opt-in-integration-tests-and-shared-fixture-lifecycle) | Internal source/context; no external report recipient identified. |
| [B-1656](https://linear.app/boundaryml2/issue/B-1656/bridge-web-generated-bytecode-traps-during-initialization-under) | Internal source/context; no external report recipient identified. |
| [B-1672](https://linear.app/boundaryml2/issue/B-1672/incident-reissue-accidentally-deleted-boundarymlbaml-actions-secrets) | Internal source/context; no external report recipient identified. |
| [B-1675](https://linear.app/boundaryml2/issue/B-1675/populate-maven-central-and-gradle-release-secrets-in-infisical) | Internal source/context; no external report recipient identified. |
| [B-1676](https://linear.app/boundaryml2/issue/B-1676/baml-fmt-does-not-handle-interface-declarations) | Internal source/context; no external report recipient identified. |
| [B-1680](https://linear.app/boundaryml2/issue/B-1680/unreflect-as-rigid-typevar-only) | Internal source/context; no external report recipient identified. |
| [B-1682](https://linear.app/boundaryml2/issue/B-1682/return-inside-a-closure-is-type-checked-against-the-enclosing-function) | Internal source/context; no external report recipient identified. |
| [B-230](https://linear.app/boundaryml2/issue/B-230/spawn-annotation-requires-exact-error-type-no-wildcard-inference) | Internal source/context; no external report recipient identified. |
| [B-236](https://linear.app/boundaryml2/issue/B-236/empty-in-ifelse-else-branch-causes-vm-crash-expected-map-got-array) | Internal source/context; no external report recipient identified. |
| [B-247](https://linear.app/boundaryml2/issue/B-247/throws-t-forces-exhaustive-re-declaration-of-stdlib-json-error-types) | Internal source/context; no external report recipient identified. |
| `B-316` | Legacy entity unavailable through the supplied key. Linked public report was read: #2736 (v0 historical). Neither is a v1 release target. |
| [B-569](https://linear.app/boundaryml2/issue/B-569/mapint-type-checks-but-crashes-at-runtime-expected-string-got-int) | Internal source/context; no external report recipient identified. |
| [B-634](https://linear.app/boundaryml2/issue/B-634/generic-match-let-s-somet-uses-an-exact-reified-type-arg-check) | Internal source/context; no external report recipient identified. |
| [B-859](https://linear.app/boundaryml2/issue/B-859/implement-bridge-web-typescript-for-wasmweb) | Internal source/context; no external report recipient identified. |
| [B-883](https://linear.app/boundaryml2/issue/B-883/codegen-type-for-never) | Internal source/context; no external report recipient identified. |
| [B-896](https://linear.app/boundaryml2/issue/B-896/associated-type-projection-through-a-generic-interface-constraint) | Internal source/context; no external report recipient identified. |
| [B-987](https://linear.app/boundaryml2/issue/B-987/type-checker-accepts-float-string-causing-vm-internal-error-at-runtime) | Internal source/context; no external report recipient identified. |
| `BAML-315` | Legacy entity unavailable through the supplied key. Linked public report was read: #1724 (v0 historical). Neither is a v1 release target. |
| `BAML-534` | Legacy entity unavailable through the supplied key. Linked public report was read: #2736 (v0 historical). Neither is a v1 release target. |
| [BAMLGH-4](https://linear.app/boundaryml2/issue/BAMLGH-4/feat-support-cloudflare-workers) | GitHub origin/context: #1259. Apply the issue disposition above. |
| [BAMLGH-44](https://linear.app/boundaryml2/issue/BAMLGH-44/sdkgen-rust-016-every-llm-function-is-skipped-implicit-aierrorsfailure) | GitHub origin/context: #4370, #4587. Apply the issue disposition above. |
| [BAMLGH-61](https://linear.app/boundaryml2/issue/BAMLGH-61/0170-throws-clause-with-an-unresolved-type-panics-the-compiler-index) | GitHub origin/context: #4587. Apply the issue disposition above. |
| [BAMLGH-62](https://linear.app/boundaryml2/issue/BAMLGH-62/baml-run-on-0170-swallows-every-thrown-error-and-prints-a-type-checker) | GitHub origin/context: #4588. Apply the issue disposition above. |
| [BAMLGH-63](https://linear.app/boundaryml2/issue/BAMLGH-63/sap-017-an-optional-class-typed-field-is-silently-coerced-to-null) | GitHub origin/context: #4589. Apply the issue disposition above. |
| [BAMLGH-70](https://linear.app/boundaryml2/issue/BAMLGH-70/official-installer-wrapper-requires-glibc-239-and-cannot-bootstrap-on) | GitHub origin/context: #4624. Apply the issue disposition above. |
| [BAMLGH-71](https://linear.app/boundaryml2/issue/BAMLGH-71/panic-in-client-cache-under-concurrent-calls-with-per-call-env-vars) | GitHub origin/context: #4641. Apply the issue disposition above. |
| [BAMLGH-73](https://linear.app/boundaryml2/issue/BAMLGH-73/bug-0170-canary-00-and-00-literals-in-the-same-function-body-alias-to) | GitHub origin/context: #4750. Apply the issue disposition above. |

## Discord evidence and disposition

| Thread | Verified source | Decision |
| --- | --- | --- |
| [Float math changes](https://discord.com/channels/1119368998161752075/1544114092044718080) | Read the August 31–September 4 conversation in the signed-in UI. Ritz links #4751, #4752, and #4750 in [the September 4 message](https://discord.com/channels/1119368998161752075/1544114092044718080/1545563183983628379). | Draft one followup to Ritz. Sam opened this thread by forwarding the external proposal. |
| [Original stdlib proposal](https://discord.com/channels/1119368998161752075/1539389233679302697) | Read Ritz’s float-method proposal and the discussion that split into the dedicated float thread. | Covered by the dedicated reply; earlier iterator work is already below the lower boundary. |
| [Transcription endpoint request](https://discord.com/channels/1119368998161752075/1357376867472248963) | Read all five messages from April 3–4, 2025. maxii asks about the v0 client<llm> transcription endpoint; linked by #1724. | Historical v0 request; no newly addressed external thread in this release. |
| [Techdocs URL availability](https://discord.com/channels/1119368998161752075/1536485344428691506) | Read August 10 question by blah28722 and August 20 response noting the improvements. | Already addressed before 0.18.0; do not notify as a new developer-portal fix. |

No additional external-started Discord thread addressed by this range was established from the PR/issue reference graph. Search candidates without report-to-fix evidence are not notification targets. Empty REST message bodies were not treated as proof that no report existed.

## Third-party reference closure

The reference graph also reaches dependency issues in the repositories below. These are dependency history and design/packaging context, not reports that this BAML release resolves. No notifications are proposed there.

- [nodejs/docker-node/issues/1794](https://github.com/nodejs/docker-node/issues/1794): read issue and comments; no new BAML fix claim.
- [nodejs/docker-node/issues/1798](https://github.com/nodejs/docker-node/issues/1798): read issue and comments; no new BAML fix claim.
- [nodejs/docker-node/issues/1829](https://github.com/nodejs/docker-node/issues/1829): read issue and comments; no new BAML fix claim.
- [nodejs/node-gyp/issues/2972](https://github.com/nodejs/node-gyp/issues/2972): read issue and comments; no new BAML fix claim.
- [npm/cli/pull/4](https://github.com/npm/cli/pull/4): read issue and comments; no new BAML fix claim.
- [npm/cli/issues/4828](https://github.com/npm/cli/issues/4828): read issue and comments; no new BAML fix claim.
- [npm/cli/issues/5743](https://github.com/npm/cli/issues/5743): read issue and comments; no new BAML fix claim.
- [npm/minipass-fetch/issues/61](https://github.com/npm/minipass-fetch/issues/61): read issue and comments; no new BAML fix claim.
- [nrwl/nx-console/issues/1808](https://github.com/nrwl/nx-console/issues/1808): read issue and comments; no new BAML fix claim.
- [nuxt/nuxt/issues/31954](https://github.com/nuxt/nuxt/issues/31954): read issue and comments; no new BAML fix claim.
- [oxc-project/oxc/issues/10822](https://github.com/oxc-project/oxc/issues/10822): read issue and comments; no new BAML fix claim.
- [oxc-project/oxc/issues/11459](https://github.com/oxc-project/oxc/issues/11459): read issue and comments; no new BAML fix claim.
- [pnpm/pnpm/issues/5965](https://github.com/pnpm/pnpm/issues/5965): read issue and comments; no new BAML fix claim.
- [rolldown/rolldown/issues/6141](https://github.com/rolldown/rolldown/issues/6141): read issue and comments; no new BAML fix claim.
- [rolldown/tsdown/issues/484](https://github.com/rolldown/tsdown/issues/484): read issue and comments; no new BAML fix claim.
- [rollup/rollup/issues/5194](https://github.com/rollup/rollup/issues/5194): read issue and comments; no new BAML fix claim.
- [rollup/rollup/issues/5571](https://github.com/rollup/rollup/issues/5571): read issue and comments; no new BAML fix claim.
- [snapview/tokio-tungstenite/issues/119](https://github.com/snapview/tokio-tungstenite/issues/119): read issue and comments; no new BAML fix claim.
- [snapview/tokio-tungstenite/issues/160](https://github.com/snapview/tokio-tungstenite/issues/160): read issue and comments; no new BAML fix claim.
- [snapview/tokio-tungstenite/issues/172](https://github.com/snapview/tokio-tungstenite/issues/172): read issue and comments; no new BAML fix claim.
- [snapview/tokio-tungstenite/issues/200](https://github.com/snapview/tokio-tungstenite/issues/200): read issue and comments; no new BAML fix claim.
- [snapview/tokio-tungstenite/issues/204](https://github.com/snapview/tokio-tungstenite/issues/204): read issue and comments; no new BAML fix claim.
- [tonistiigi/binfmt/issues/197](https://github.com/tonistiigi/binfmt/issues/197): read issue and comments; no new BAML fix claim.
- [vercel/turborepo/issues/3328](https://github.com/vercel/turborepo/issues/3328): read issue and comments; no new BAML fix claim.
- [vercel/turborepo/issues/2517](https://github.com/vercel/turborepo/issues/2517): read issue and comments; no new BAML fix claim.
- [vercel/turborepo/issues/3328](https://github.com/vercel/turborepo/issues/3328): read issue and comments; no new BAML fix claim.

## Drafts and publication dependencies

[0.19.0.todo.md](../typescript2/app-website/blog-releases/0.19.0.todo.md) contains six notification drafts: two contributor PRs, two external-opened issues, one additional maintainer-opened issue with external participants, and one dedicated Discord reply. Every draft links to https://boundaryml.com/changelog. The release date/version are provisional. A wrapper version bump, publication, and bootstrap verification gate both the installer changelog claim and #4624; all messages remain unsent.
