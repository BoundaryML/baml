# BAML 0.24.0 follow-up source audit

The fixed lower release boundary is `f0f0bb5135bba599c11a32cc4d631f2f66f1f96f` (`baml-language-0.23.0`). The fixed upper release boundary is `dec4713a0e7c9699442bf5855b4f327063f769c5` (`baml-language-0.24.0`). The published GitHub release targets that upper commit. This audit does not include later changes on `origin/canary`.

## Coverage

All 84 commits in this range are squash-merged PRs. GitHub metadata, descriptions, issue comments, reviews, inline review threads, linked closing issues, and changed-file paths were retrieved for all 84. The notification audit includes the 81 v1 PRs, including CI, workflow, marketing, and documentation PRs with no public language-release entry. Only the three exclusively v0 PRs listed below are excluded.

There were 959 issue-comment/review bodies and 389 inline comments. All comment, review, review-thread, and thread-comment connections were exhausted. Changed-file lists exceeded the initial 100-path page for #5359, #5513, #5519, and #5528; notification identity and reference scans did not depend on those lists. Local merged diffs supply complete file inventories for release classification.

57 additional contextual GitHub issues/PRs were fetched with their descriptions and issue comments. This includes bot-suggested associations; an association was not accepted as evidence that a report was fixed. Short `#number` strings occurring in patch excerpts also resolve to old unrelated issues and do not expand the release boundary. GitHub #5464 was unavailable; the in-range reference is a contextual PR number, not evidence of a shipped fix or a notification recipient. Six TASK identifiers in PR prose (TASK-18, TASK-174, TASK-175, TASK-181, TASK-198, TASK-201) could not be resolved as Linear issues. They are not fabricated into reporter identities.

38 Linear issues were read with attachment metadata and all 37 comments, covering direct links, GitHub report mirrors, and relevant linked context. Sources identified internal authors, mirrored the external GitHub reports, or described changes from earlier releases. No audited source linked a Discord thread. No Discord conversation or recipient is inferred from absent evidence. Credentials were retrieved from the specified Infisical environment in process memory; no credentials or raw private source dumps belong in this repository.

## Attribution and exclusions

| Source | Decision | Evidence and scope |
| --- | --- | --- |
| #5264, #5442, #5498, #5503, #5506, #5531 | External collaborator: @harivansh-afk. | Native compilation, its differential CI coverage, escape analysis, optimized array iteration, native helper inlining, and the finalizer safety correction are present at the upper boundary. CI-only #5442 gets an acknowledgment without claiming an SDK feature. |
| [#4377](https://github.com/BoundaryML/baml/issues/4377) | External reporter: @BenSpex. | #5509 stops baml_bridge from forcing serde_json arbitrary_precision and preserve_order. The migration requires explicit feature opt-in to retain the earlier JSON behavior. |
| [#5074](https://github.com/BoundaryML/baml/issues/5074) / BAMLGH-93 | External reporter: @BenSpex. | #5426/#5422 select the target project before toolchain resolution. At the upper boundary `WrapperInvocation.parse` derives `project` from --project and `selection()` searches that project. The fix requires wrapper 0.3.0; a toolchain-only update leaves the old wrapper in place. BAML_TOOLCHAIN keeps precedence. |
| [#5092](https://github.com/BoundaryML/baml/issues/5092) / BAMLGH-97 | Do not announce as addressed. | A bot associates wrapper conformance tests with the report, but those tests do not expose the selected project root or current executable to user test code. |
| [#5392](https://github.com/BoundaryML/baml/issues/5392) / BAMLGH-118 | Do not announce a new 0.24.0 fix. | #5456 adds subprocess lifetime controls to user code. It does not change the CLI's own stdout ownership or telemetry shutdown budget. Earlier bounded-shutdown improvements were already in 0.23.0. @duongynhi000005-oss contributed an investigation on the issue, but its proposed additional fix is not shipped in this range. |
| [#5441](https://github.com/BoundaryML/baml/issues/5441) / BAMLGH-119 | Do not announce as addressed. | #5520 changes homepage illustrations. It does not restore query columns, change SQL indexing, add cost data, or repair the cloud query schema. |
| [#5444](https://github.com/BoundaryML/baml/issues/5444) / BAMLGH-121 | Do not announce as addressed. | Homepage dashboard marketing does not repair grant expiry, budget reporting, or cloud query performance. |
| #5416 | No release notification. | External author saruar-dev's NaceAI PR is referenced contextually by #5479. It is outside the pinned release inventory; removing a linked-source dependency does not ship that provider PR. |
| BAMLGH-75 / #4765, BAMLGH-91, B-1823 | No new-release notification. | The fetched context describes strict escapes and unit/test-body behavior already shipped before the lower boundary. |
| B-2012, B-2013 | Do not announce as fixed. | Fork-after-initialization hangs and busy-loop Python callback GIL handoff are explicitly open follow-ups to #5508. Its completion-result delivery change does not fix either. |
| B-1996, B-1999, B-2002 | Do not announce as fixed. | Per-list union fallback hints, repeated custom LLM conversion during streaming, and opaque-handle get_field behavior remain distinct follow-ups. |

The only external human found in the in-range PR discussions was @harivansh-afk. Other human participants had MEMBER association. Bot accounts are not notification recipients. Adopted predecessor commits in the wrapper stack retain internal authorship.

The action checklist was removed. This file retains attribution and issue-scope evidence; no notifications were sent.

## Linear sources

| Issue | Title | Origin |
| --- | --- | --- |
| [B-1121](https://linear.app/boundaryml2/issue/B-1121/baml-generate-points-to-the-wrong-package) | baml generate points to the wrong package | Sam Lijin |
| [B-1151](https://linear.app/boundaryml2/issue/B-1151/empty-union-variant-swallows-later-json-union-variants) | Empty union variant swallows later JSON union variants | Aaron Villalpando |
| [B-1555](https://linear.app/boundaryml2/issue/B-1555/to-jsonx-omit-null-fields-true-serializer-level-null-omission-to) | to_json(x, omit_null_fields = true): serializer-level null omission to replace five hand-rolled _prune_nulls | Aaron Villalpando |
| [B-1604](https://linear.app/boundaryml2/issue/B-1604/make-sap-a-vm-function-instead-of-sysop) | Make SAP a VM function instead of sysop | Kai Orita |
| [B-1641](https://linear.app/boundaryml2/issue/B-1641/bamljson-methods-cleanup) | `baml.json.*` methods cleanup | Kai Orita |
| [B-1722](https://linear.app/boundaryml2/issue/B-1722/a-bare-function-type-in-a-non-first-union-position-silently-loses-its) | A bare function type in a non-first union position silently loses its arrow | Kai Orita |
| [B-1807](https://linear.app/boundaryml2/issue/B-1807/xnn-unnnn-u-and-unknown-escapes-are-kept-verbatim-with-no-diagnostic) | `\xNN`, `\uNNNN`, `\u{...}` and unknown escapes are kept verbatim, with no diagnostic | Sam Lijin |
| [B-1815](https://linear.app/boundaryml2/issue/B-1815/bamljsonfrom-string-jsonparse-are-3-slower-than-jsonparse-hs-11) | `baml.json.from_string` / `json.parse` are ~3× slower than `JSON.parse` (H's 11× included per-line string building) | Sam Lijin |
| [B-1816](https://linear.app/boundaryml2/issue/B-1816/json-output-prints-integral-floats-as-10-pretty-print-indent-is-fixed) | JSON output prints integral floats as `1.0`; pretty-print indent is fixed at 2 spaces | Sam Lijin |
| [B-1818](https://linear.app/boundaryml2/issue/B-1818/naninf-cannot-go-through-the-json-bridge-to-string-writes-null-from) | NaN/±Inf cannot go through the JSON bridge (`to_string` writes `null`, `from_json` rejects them) | Sam Lijin |
| [B-1822](https://linear.app/boundaryml2/issue/B-1822/alias-is-ignored-by-bamljson) | `@alias` is ignored by `baml.json` | Sam Lijin |
| [B-1823](https://linear.app/boundaryml2/issue/B-1823/a-test-block-whose-last-expression-is-false-passes) | A `test` block whose last expression is `false` passes | Sam Lijin |
| [B-1825](https://linear.app/boundaryml2/issue/B-1825/bamljsonstringifyunknown-is-rejected) | `baml.json.stringify(unknown)` is rejected | Sam Lijin |
| [B-1831](https://linear.app/boundaryml2/issue/B-1831/a-statement-starting-with-or-after-a-block-is-parsed-as-a-continuation) | A statement starting with `[`, `-` or `(` after a block is parsed as a continuation of the block | Sam Lijin |
| [B-1836](https://linear.app/boundaryml2/issue/B-1836/interpreter-baseline-50-250-slower-than-gonodecpython-on-real-ports) | Interpreter baseline: 50–250× slower than Go/Node/CPython on real ports; string interpolation and `to_string` are the standout hot spot | Sam Lijin |
| [B-1841](https://linear.app/boundaryml2/issue/B-1841/bamljsonfrom-stringt-into-a-union-picks-the-first-member-that-decodes) | `baml.json.from_string<T>` into a union picks the first member that decodes | Sam Lijin |
| [B-1868](https://linear.app/boundaryml2/issue/B-1868/assigning-a-bound-method-into-a-field-or-map-slot-panics-the-compiler) | Assigning a bound method into a field or map slot panics the compiler | Sam Lijin |
| [B-1902](https://linear.app/boundaryml2/issue/B-1902/no-reflective-field-setting-no-struct-tags-custom-attributes) | No reflective field setting; no struct tags / custom attributes | Sam Lijin |
| [B-1913](https://linear.app/boundaryml2/issue/B-1913/a-field-typed-json-unknown-must-be-present-even-though-the-type) | A field typed `json` / `unknown` must be present even though the type includes `null` | Sam Lijin |
| [B-1992](https://linear.app/boundaryml2/issue/B-1992/a-union-with-a-uint8array-member-is-rejected-as-an-llm-output-in-sap) | A union with a `uint8array` member is rejected as an LLM output, in SAP and in tool arguments | Antonio Sarosi |
| [B-1993](https://linear.app/boundaryml2/issue/B-1993/bamljsonschema-describes-a-type-with-a-json-override-by-its-fields-not) | `baml.json.schema` describes a type with a JSON override by its fields, not by the override | Antonio Sarosi |
| [B-1994](https://linear.app/boundaryml2/issue/B-1994/cache-tojsonfromjson-override-lookups-across-calls) | Cache ToJson/FromJson override lookups across calls | Antonio Sarosi |
| [B-1995](https://linear.app/boundaryml2/issue/B-1995/assert-that-cst-to-ast-lowering-does-not-silently-lose-syntax) | Assert that CST-to-AST lowering does not silently lose syntax | Sam Lijin |
| [B-1996](https://linear.app/boundaryml2/issue/B-1996/sap-reads-list-items-after-a-string-fallback-as-strings-even-when-an) | SAP reads list items after a string fallback as strings, even when an earlier union member fits | Antonio Sarosi |
| [B-1999](https://linear.app/boundaryml2/issue/B-1999/streaming-calls-from-llm-output-again-for-every-complete-value-on) | Streaming calls `from_llm_output` again for every complete value on every chunk | Antonio Sarosi |
| [B-2000](https://linear.app/boundaryml2/issue/B-2000/each-new-runtime-classs-first-interface-lookup-scans-every-minted) | Each new runtime class's first interface lookup scans every minted class's witnesses | Antonio Sarosi |
| [B-2002](https://linear.app/boundaryml2/issue/B-2002/reflectclassget-fieldunknown-on-a-field-holding-a-native-handle-throws) | `reflect.Class.get_field<unknown>` on a field holding a native handle throws a confusing CompilationError | Antonio Sarosi |
| [B-2004](https://linear.app/boundaryml2/issue/B-2004/a-continuation-line-starting-with-is-silently-dropped-as-its-own) | A continuation line starting with `-` is silently dropped as its own statement after #5357 (no warning) | Antonio Sarosi |
| [B-2012](https://linear.app/boundaryml2/issue/B-2012/baml-calls-in-a-forked-child-process-hang-or-lose-telemetry) | BAML calls in a forked child process hang or lose telemetry | Antonio Sarosi |
| [B-2013](https://linear.app/boundaryml2/issue/B-2013/python-callbacks-in-async-baml-calls-likely-wait-seconds-for-the-gil) | Python callbacks in async BAML calls likely wait seconds for the GIL when the event loop is busy | Antonio Sarosi |
| [B-202](https://linear.app/boundaryml2/issue/B-202/sap-coercion-interface) | SAP coercion interface | Aaron Villalpando |
| [BAMLGH-118](https://linear.app/boundaryml2/issue/BAMLGH-118/baml-run-with-cloud-tracing-on-keeps-stdout-open-and-the-process-alive) | `baml run` with cloud tracing on keeps stdout open and the process alive until the trace upload finishes, so callers block for the whole upload | GitHub mirror |
| [BAMLGH-119](https://linear.app/boundaryml2/issue/BAMLGH-119/bug-baml-query-spanstemporary-projections-removed-without-a-changelog) | [bug] baml query: spans.temporary_projections removed without a changelog; self_usage[1]['field'] silently returns NULL | GitHub mirror |
| [BAMLGH-121](https://linear.app/boundaryml2/issue/BAMLGH-121/studio-dashboards-windowboundaryquery-fails-with-expired-data-plane) | Studio dashboards: window.boundary.query fails with 'expired data-plane access' / 'truncated' at ~300k spans/day — budgets and grant lifetime undocumented | GitHub mirror |
| [BAMLGH-75](https://linear.app/boundaryml2/issue/BAMLGH-75/string-literals-do-not-decode-xhh-uhhhh-escapes-kept-raw) | String literals do not decode \xHH / \uHHHH escapes (kept raw) | GitHub mirror |
| [BAMLGH-91](https://linear.app/boundaryml2/issue/BAMLGH-91/handle-return-values-from-void-functions-in-baml) | Handle return values from void functions in BAML | paulo |
| [BAMLGH-93](https://linear.app/boundaryml2/issue/BAMLGH-93/baml-project-dir-uses-the-cwds-toolchain-pin-instead-of-the-target) | baml --project <dir> uses the cwd's toolchain pin instead of the target project's pin | GitHub mirror |
| [BAMLGH-97](https://linear.app/boundaryml2/issue/BAMLGH-97/feature-baml-cli-test-bamlsysargv-is-empty-and-test-code-cannot-learn) | [feature] baml-cli test: baml.sys.argv() is empty and test code cannot learn the running engine or selected --project root | GitHub mirror |

## Complete PR inventory

“V1 audited” includes changes omitted from the public changelog. This table records notification coverage, not user-visible effect classification.

| Commit | PR | Author | Scope |
| --- | --- | --- | --- |
| `7bce4770de994a0766e754901f376972bec4a601` | [#5264](https://github.com/BoundaryML/baml/pull/5264) | harivansh-afk | V1 audited |
| `8c4c6a155271a4804b92a45fa38f813a16289dbc` | [#5412](https://github.com/BoundaryML/baml/pull/5412) | aaronvg | V1 audited |
| `27a1b2624f91c4f00dafbb43afaec77c38f3ef17` | [#5417](https://github.com/BoundaryML/baml/pull/5417) | antoniosarosi | V1 audited |
| `5c9e2531cdc2c9e70d8f563c1536bc03170de0dd` | [#5418](https://github.com/BoundaryML/baml/pull/5418) | antoniosarosi | V1 audited |
| `e9e593a207b1a2f00e05d832f17c44c962a69097` | [#5414](https://github.com/BoundaryML/baml/pull/5414) | antoniosarosi | V1 audited |
| `74d6ab7bf8779c73068f366f16419861b2d3bd9e` | [#5415](https://github.com/BoundaryML/baml/pull/5415) | antoniosarosi | V1 audited |
| `166bbc07042699e5a21428524223ac88150e9238` | [#5420](https://github.com/BoundaryML/baml/pull/5420) | antoniosarosi | V1 audited |
| `019a6cc2be572d552f1759fc52dd6fa06c6efb09` | [#5385](https://github.com/BoundaryML/baml/pull/5385) | sxlijin | V1 audited |
| `3d20df898fbea29e24be06222aeee25ad48f8597` | [#5429](https://github.com/BoundaryML/baml/pull/5429) | antoniosarosi | V1 audited |
| `63f9cb3a9e041d4581c6931dd32ec5346b1ce1db` | [#5430](https://github.com/BoundaryML/baml/pull/5430) | antoniosarosi | V1 audited |
| `4dbe99aa6789fddc675dd07febaa9f24efcf2998` | [#5431](https://github.com/BoundaryML/baml/pull/5431) | antoniosarosi | V1 audited |
| `ce992b882d79ea46d0c6de162db48480218c685a` | [#5432](https://github.com/BoundaryML/baml/pull/5432) | antoniosarosi | V1 audited |
| `e9e94256c54771f81b08acd6b93e2259814eb283` | [#5433](https://github.com/BoundaryML/baml/pull/5433) | antoniosarosi | V1 audited |
| `18ec299d068092b934a390b6040f156ac2de362f` | [#5435](https://github.com/BoundaryML/baml/pull/5435) | aaronvg | V0-only; excluded |
| `151d33cb591f17e4995c4237f00f279482a01dc3` | [#5436](https://github.com/BoundaryML/baml/pull/5436) | antoniosarosi | V1 audited |
| `ecb640608f730191e07b9122cab99a521e7b178d` | [#5437](https://github.com/BoundaryML/baml/pull/5437) | aaronvg | V0-only; excluded |
| `051e489cb06637765c8e22876191ddc5a16468f9` | [#5359](https://github.com/BoundaryML/baml/pull/5359) | 2kai2kai2 | V1 audited |
| `9574eda7142eabc9f9172df3089ce9f8a9ecfde7` | [#5439](https://github.com/BoundaryML/baml/pull/5439) | sxlijin | V1 audited |
| `9bc29b9619a32b78d595533e907f50b3e2875043` | [#5442](https://github.com/BoundaryML/baml/pull/5442) | harivansh-afk | V1 audited |
| `98502d28a8350c4e13298992648b6bda97081cc0` | [#5440](https://github.com/BoundaryML/baml/pull/5440) | aaronvg | V0-only; excluded |
| `9ccbcebad4bd0081fcfce602bf5a2550836462f6` | [#5423](https://github.com/BoundaryML/baml/pull/5423) | 2kai2kai2 | V1 audited |
| `1d4ee52e6aefbf7bc5207eb783786597daffd836` | [#5424](https://github.com/BoundaryML/baml/pull/5424) | codeshaunted | V1 audited |
| `9eb13097920e1e442ef0c9f8235d9a07a662c843` | [#5446](https://github.com/BoundaryML/baml/pull/5446) | codeshaunted | V1 audited |
| `bd1d44ccecfba655a5ec9a6a217f6e38a4432869` | [#5449](https://github.com/BoundaryML/baml/pull/5449) | codeshaunted | V1 audited |
| `464388f729830d5ebee58ba0d345361e80e681cf` | [#5448](https://github.com/BoundaryML/baml/pull/5448) | codeshaunted | V1 audited |
| `e27aceb1b466057fce5d2901d735482205520c8e` | [#5452](https://github.com/BoundaryML/baml/pull/5452) | codeshaunted | V1 audited |
| `94a92a92bca79852231c829115256ebf8bb07c73` | [#5453](https://github.com/BoundaryML/baml/pull/5453) | codeshaunted | V1 audited |
| `654cb9b57bbd84a19225af3465afee06e2f78c6c` | [#5455](https://github.com/BoundaryML/baml/pull/5455) | codeshaunted | V1 audited |
| `478fcc035721c623e79d1ba82cb9b2310c285e00` | [#5466](https://github.com/BoundaryML/baml/pull/5466) | codeshaunted | V1 audited |
| `1d771278eceab6a351ba2e6b0331559261dc55fe` | [#5468](https://github.com/BoundaryML/baml/pull/5468) | codeshaunted | V1 audited |
| `704908afe94161c75a9965889103181e2f8f5f24` | [#5467](https://github.com/BoundaryML/baml/pull/5467) | codeshaunted | V1 audited |
| `ae746e18f7aeabcdb79cafe6e3f67b3a35caaf13` | [#5470](https://github.com/BoundaryML/baml/pull/5470) | codeshaunted | V1 audited |
| `59777c8a76477a69853e898eb211d542c0b054ab` | [#5471](https://github.com/BoundaryML/baml/pull/5471) | 2kai2kai2 | V1 audited |
| `1ace85eae5155b89b1e7e98209d80381834caa39` | [#5477](https://github.com/BoundaryML/baml/pull/5477) | antoniosarosi | V1 audited |
| `119cea06229606e27b8ce3abc73b372ea7177b69` | [#5478](https://github.com/BoundaryML/baml/pull/5478) | codeshaunted | V1 audited |
| `ff8fddbef185b65d519b45ca8fbf09c1e5ba5623` | [#5479](https://github.com/BoundaryML/baml/pull/5479) | codeshaunted | V1 audited |
| `aa80f5197dd30da8955af5901c9e4608573a2819` | [#5481](https://github.com/BoundaryML/baml/pull/5481) | 2kai2kai2 | V1 audited |
| `40445fcff5dc631345b553010f9dfad927269202` | [#5480](https://github.com/BoundaryML/baml/pull/5480) | codeshaunted | V1 audited |
| `2965d42280640b6173cf41f126d236bce4060693` | [#5482](https://github.com/BoundaryML/baml/pull/5482) | codeshaunted | V1 audited |
| `01b8eb8daaf1a335340c483b7294b1e1966f4154` | [#5486](https://github.com/BoundaryML/baml/pull/5486) | codeshaunted | V1 audited |
| `9ffcb63b87a7d6edf7c0009820729debf9581394` | [#5485](https://github.com/BoundaryML/baml/pull/5485) | codeshaunted | V1 audited |
| `4ef24545c25bc92df11279692aee01159396abe2` | [#5487](https://github.com/BoundaryML/baml/pull/5487) | codeshaunted | V1 audited |
| `54374938bf7f2cd59aa8cdfad300234d86fc08b3` | [#5489](https://github.com/BoundaryML/baml/pull/5489) | sxlijin | V1 audited |
| `4b1f509556521a41c506e272759da5379b2f80f7` | [#5484](https://github.com/BoundaryML/baml/pull/5484) | codeshaunted | V1 audited |
| `85da331a7d2881696dbb9e64d2d1782cc29993a0` | [#5490](https://github.com/BoundaryML/baml/pull/5490) | sxlijin | V1 audited |
| `ecd5f276597b978ba722e0513394afc7736a8057` | [#5425](https://github.com/BoundaryML/baml/pull/5425) | sxlijin | V1 audited |
| `b355d3369c2ec2106ea767b43b3af1bd518f4f66` | [#5426](https://github.com/BoundaryML/baml/pull/5426) | sxlijin | V1 audited |
| `591d6d1de2e96549d6c480652a069ff89786e700` | [#5456](https://github.com/BoundaryML/baml/pull/5456) | sxlijin | V1 audited |
| `75337011b96811c79bc60031d1cd958acdfd3788` | [#5457](https://github.com/BoundaryML/baml/pull/5457) | sxlijin | V1 audited |
| `0dc73a9de9b00ac3d693c9451921bd1703f1c010` | [#5463](https://github.com/BoundaryML/baml/pull/5463) | sxlijin | V1 audited |
| `857c3f247b3d7afdc50bf469ee7c096bc702b930` | [#5422](https://github.com/BoundaryML/baml/pull/5422) | sxlijin | V1 audited |
| `aae6e9179db3731170ac53243a476725aa155d04` | [#5496](https://github.com/BoundaryML/baml/pull/5496) | antoniosarosi | V1 audited |
| `261588e192b27feafbfb5193ee8dd43df558b593` | [#5502](https://github.com/BoundaryML/baml/pull/5502) | codeshaunted | V1 audited |
| `b092354563ef7d4cc82d4ac4c354583f1ea4f1e9` | [#5497](https://github.com/BoundaryML/baml/pull/5497) | antoniosarosi | V1 audited |
| `ab5042f98b1cb9d5052a8925fd8178431417d96a` | [#5500](https://github.com/BoundaryML/baml/pull/5500) | codeshaunted | V1 audited |
| `06e0854b7c7497212e035a7b94057714c2fd16d5` | [#5494](https://github.com/BoundaryML/baml/pull/5494) | 2kai2kai2 | V1 audited |
| `253082af8ea004ed3b2f1eeb6a69d244fa7b86b4` | [#5509](https://github.com/BoundaryML/baml/pull/5509) | antoniosarosi | V1 audited |
| `78b539244cbc095dec1d4ab624adc56e6d1d5ce0` | [#5513](https://github.com/BoundaryML/baml/pull/5513) | aaronvg | V1 audited |
| `a59d79d430b9c99937ca4e852bb641c6efb16361` | [#5491](https://github.com/BoundaryML/baml/pull/5491) | sxlijin | V1 audited |
| `a2ff141306c34c9339455b4626f72fe0915c9160` | [#5515](https://github.com/BoundaryML/baml/pull/5515) | sxlijin | V1 audited |
| `8df529f9d19f80167f84be1a36b643ae3e05874b` | [#5514](https://github.com/BoundaryML/baml/pull/5514) | sxlijin | V1 audited |
| `7c55cb9cb0f4b31c8bea3007cdd44ef8c18fa86e` | [#5511](https://github.com/BoundaryML/baml/pull/5511) | antoniosarosi | V1 audited |
| `39a1de3b070bd7ceeb5cd7fc6dd8c5b656e5dd65` | [#5508](https://github.com/BoundaryML/baml/pull/5508) | antoniosarosi | V1 audited |
| `a6457173ef1faeb5fae6bdccd7b4c396b7e24ada` | [#5517](https://github.com/BoundaryML/baml/pull/5517) | aaronvg | V1 audited |
| `e1d48907d7dfb28e39c0fbbffa51d57d7d2d3168` | [#5518](https://github.com/BoundaryML/baml/pull/5518) | antoniosarosi | V1 audited |
| `91e0ddf196e8c37c8507d69b289c03520aa09439` | [#5521](https://github.com/BoundaryML/baml/pull/5521) | 2kai2kai2 | V1 audited |
| `c4c12768489d391b3d0ece87a5f14162f037470a` | [#5520](https://github.com/BoundaryML/baml/pull/5520) | aaronvg | V1 audited |
| `2d56c4ddec6075f22f6cef3f6d3e55c241fd8606` | [#5516](https://github.com/BoundaryML/baml/pull/5516) | sxlijin | V1 audited |
| `e8157fd049381b6214a5545ac9e4f58a8604cc10` | [#5519](https://github.com/BoundaryML/baml/pull/5519) | antoniosarosi | V1 audited |
| `240c120465e6fe20924f9f84d0a76424b8cf5aa7` | [#5498](https://github.com/BoundaryML/baml/pull/5498) | harivansh-afk | V1 audited |
| `dfa138c19a965a7a3d9ef7bec1a1880b05317d75` | [#5503](https://github.com/BoundaryML/baml/pull/5503) | harivansh-afk | V1 audited |
| `7558206a1802c794c7e852c52016f3ae83500d44` | [#5506](https://github.com/BoundaryML/baml/pull/5506) | harivansh-afk | V1 audited |
| `842da5e2edca55590fb48ac78c2d95fc874952f1` | [#5524](https://github.com/BoundaryML/baml/pull/5524) | antoniosarosi | V1 audited |
| `aabd3533218619285a461ace5bf11dd6b482ead9` | [#5525](https://github.com/BoundaryML/baml/pull/5525) | codeshaunted | V1 audited |
| `462088f8427313d68f504bec925d5e82d7fe149a` | [#5526](https://github.com/BoundaryML/baml/pull/5526) | antoniosarosi | V1 audited |
| `a6d8fcd4700a83739971a26a71b75469551d5f60` | [#5528](https://github.com/BoundaryML/baml/pull/5528) | aaronvg | V1 audited |
| `c83cef10dd18741b47c5a3fb5d2334d076ff6e17` | [#5529](https://github.com/BoundaryML/baml/pull/5529) | codeshaunted | V1 audited |
| `8eb462d8592848b3a81dbf4396776361091c8d6b` | [#5527](https://github.com/BoundaryML/baml/pull/5527) | antoniosarosi | V1 audited |
| `a5db82f06c1da0dfac28e283a15d19d37a3f2cf2` | [#5531](https://github.com/BoundaryML/baml/pull/5531) | harivansh-afk | V1 audited |
| `c3ff351043929f9985f548bd2427207d95ec9492` | [#5535](https://github.com/BoundaryML/baml/pull/5535) | codeshaunted | V1 audited |
| `c02edfc5399d0e11bd20aa6140cfb0671379a930` | [#5537](https://github.com/BoundaryML/baml/pull/5537) | codeshaunted | V1 audited |
| `a4ba1b31be27414f79675290c93db02448c5de74` | [#5541](https://github.com/BoundaryML/baml/pull/5541) | codeshaunted | V1 audited |
| `49d6ba488e3cd2ccbd17448bb3d7f8589404da06` | [#5542](https://github.com/BoundaryML/baml/pull/5542) | codeshaunted | V1 audited |
| `dec4713a0e7c9699442bf5855b4f327063f769c5` | [#5483](https://github.com/BoundaryML/baml/pull/5483) | sxlijin | V1 audited |
