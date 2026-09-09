# Complete release-range inventory

Range: `baml-language-0.17.0..baml-language-0.18.0` (lower tag excluded; upper tag included). Sources were inspected at the upper tag, not current canary.

The range contains 114 commits/PRs. The procedure’s path-filtered log returns 89. Only four PRs are BAML v0-only; the followup inventory therefore includes 110 PRs. `Keep` means a net user-visible effect in this draft. The workflow-only musl fix #4502 is an explicitly documented correction to the procedure’s filter.

| PR | Language path? | Decision | Evidence / reason |
| --- | --- | --- | --- |
| [#4455](https://github.com/BoundaryML/baml/pull/4455) | Yes | Exclude | Documentation for the preceding release. |
| [#4453](https://github.com/BoundaryML/baml/pull/4453) | Yes | Keep | C02 |
| [#4458](https://github.com/BoundaryML/baml/pull/4458) | Yes | Keep | C03 |
| [#4460](https://github.com/BoundaryML/baml/pull/4460) | Yes | Keep | C33 |
| [#4461](https://github.com/BoundaryML/baml/pull/4461) | Yes | Exclude | Internal cache reuse; its own benchmarks explicitly claim no end-to-end speedup. |
| [#4462](https://github.com/BoundaryML/baml/pull/4462) | Yes | Exclude | CI fixture scheduling only. |
| [#4463](https://github.com/BoundaryML/baml/pull/4463) | Yes | Keep | C03 |
| [#4464](https://github.com/BoundaryML/baml/pull/4464) | Yes | Exclude | Entire change reverted by #4469 before the upper tag. |
| [#4469](https://github.com/BoundaryML/baml/pull/4469) | Yes | Exclude | Cancels #4464; no net release change. |
| [#4456](https://github.com/BoundaryML/baml/pull/4456) | Yes | Exclude | Changelog cleanup and release notifications. |
| [#4202](https://github.com/BoundaryML/baml/pull/4202) | No | Exclude | BAML v0 audio provider in engine/. |
| [#4407](https://github.com/BoundaryML/baml/pull/4407) | No | Exclude | Contributor command for installing a locally built VS Code extension. |
| [#4408](https://github.com/BoundaryML/baml/pull/4408) | Yes | Keep | C32 |
| [#4472](https://github.com/BoundaryML/baml/pull/4472) | Yes | Exclude | C# release size-gate policy only. |
| [#4409](https://github.com/BoundaryML/baml/pull/4409) | Yes | Keep | C31 |
| [#4474](https://github.com/BoundaryML/baml/pull/4474) | No | Exclude | BAML v0 version bump. |
| [#4471](https://github.com/BoundaryML/baml/pull/4471) | No | Exclude | CI baseline automation only. |
| [#4378](https://github.com/BoundaryML/baml/pull/4378) | Yes | Exclude | Private Ruby loader groundwork; no callable end-user Ruby SDK introduced. |
| [#4466](https://github.com/BoundaryML/baml/pull/4466) | Yes | Keep | C34 |
| [#4465](https://github.com/BoundaryML/baml/pull/4465) | Yes | Exclude | Binary-size baseline refresh only. |
| [#4478](https://github.com/BoundaryML/baml/pull/4478) | Yes | Keep | C35 |
| [#4489](https://github.com/BoundaryML/baml/pull/4489) | Yes | Keep | C36 |
| [#4467](https://github.com/BoundaryML/baml/pull/4467) | Yes | Keep | C37 |
| [#4470](https://github.com/BoundaryML/baml/pull/4470) | Yes | Keep | C38 |
| [#4473](https://github.com/BoundaryML/baml/pull/4473) | Yes | Keep | C39 |
| [#4491](https://github.com/BoundaryML/baml/pull/4491) | Yes | Keep | C10 C40 |
| [#4494](https://github.com/BoundaryML/baml/pull/4494) | No | Exclude | Repository editor preference only. |
| [#4493](https://github.com/BoundaryML/baml/pull/4493) | Yes | Keep | C10 C40 |
| [#4490](https://github.com/BoundaryML/baml/pull/4490) | Yes | Keep | C41 |
| [#4495](https://github.com/BoundaryML/baml/pull/4495) | Yes | Keep | C42 |
| [#4499](https://github.com/BoundaryML/baml/pull/4499) | Yes | Exclude | Test timing and hermeticity only. |
| [#4501](https://github.com/BoundaryML/baml/pull/4501) | Yes | Keep | C43 |
| [#4498](https://github.com/BoundaryML/baml/pull/4498) | Yes | Keep | C12 |
| [#4503](https://github.com/BoundaryML/baml/pull/4503) | No | Exclude | BAML v0 OpenAI token-detail fix in engine/. |
| [#4518](https://github.com/BoundaryML/baml/pull/4518) | Yes | Keep | C45 |
| [#4524](https://github.com/BoundaryML/baml/pull/4524) | No | Exclude | BAML v0 version bump. |
| [#4516](https://github.com/BoundaryML/baml/pull/4516) | Yes | Keep | C44 |
| [#4528](https://github.com/BoundaryML/baml/pull/4528) | No | Exclude | CI protoc installation only. |
| [#4519](https://github.com/BoundaryML/baml/pull/4519) | Yes | Exclude | Generic-function descriptor listing and specialize/get APIs were removed by #4560 before the upper tag; no shipped feature. |
| [#4529](https://github.com/BoundaryML/baml/pull/4529) | Yes | Keep | C46 |
| [#4479](https://github.com/BoundaryML/baml/pull/4479) | No | Exclude | Contributor/CI sccache logging configuration. |
| [#4517](https://github.com/BoundaryML/baml/pull/4517) | Yes | Exclude | Test cleanup only. |
| [#4481](https://github.com/BoundaryML/baml/pull/4481) | Yes | Exclude | TypeScript declaration-test harness speed only. |
| [#4533](https://github.com/BoundaryML/baml/pull/4533) | Yes | Exclude | Python test setup build deduplication only. |
| [#4531](https://github.com/BoundaryML/baml/pull/4531) | Yes | Keep | C47 |
| [#4530](https://github.com/BoundaryML/baml/pull/4530) | Yes | Keep | C45 |
| [#4441](https://github.com/BoundaryML/baml/pull/4441) | Yes | Keep | C15 |
| [#4502](https://github.com/BoundaryML/baml/pull/4502) | No | Keep | C30 |
| [#4535](https://github.com/BoundaryML/baml/pull/4535) | Yes | Keep | C28 |
| [#4459](https://github.com/BoundaryML/baml/pull/4459) | Yes | Keep | C07 C08 C09 C38 C66 |
| [#4526](https://github.com/BoundaryML/baml/pull/4526) | Yes | Keep | C48 |
| [#4537](https://github.com/BoundaryML/baml/pull/4537) | No | Exclude | Non-gating CI runner preview only. |
| [#4539](https://github.com/BoundaryML/baml/pull/4539) | No | Exclude | CI runner/cache isolation only. |
| [#4510](https://github.com/BoundaryML/baml/pull/4510) | Yes | Keep | C16 |
| [#4536](https://github.com/BoundaryML/baml/pull/4536) | Yes | Keep | C44 |
| [#4500](https://github.com/BoundaryML/baml/pull/4500) | Yes | Keep | C13 |
| [#4508](https://github.com/BoundaryML/baml/pull/4508) | Yes | Keep | C49 |
| [#4544](https://github.com/BoundaryML/baml/pull/4544) | Yes | Keep | C49 |
| [#4522](https://github.com/BoundaryML/baml/pull/4522) | Yes | Keep | C28 |
| [#4547](https://github.com/BoundaryML/baml/pull/4547) | Yes | Keep | C50 |
| [#4548](https://github.com/BoundaryML/baml/pull/4548) | Yes | Keep | C01 |
| [#4541](https://github.com/BoundaryML/baml/pull/4541) | Yes | Keep | C29 C32 C36 C51 C69 |
| [#4552](https://github.com/BoundaryML/baml/pull/4552) | No | Exclude | CI runner provisioning only. |
| [#4555](https://github.com/BoundaryML/baml/pull/4555) | No | Exclude | Internal oncall schedule only. |
| [#4543](https://github.com/BoundaryML/baml/pull/4543) | Yes | Keep | C22 |
| [#4554](https://github.com/BoundaryML/baml/pull/4554) | Yes | Exclude | Binary-size baseline refresh only. |
| [#4560](https://github.com/BoundaryML/baml/pull/4560) | Yes | Exclude | Runtime representation rewrite also removes the unreleased #4519 API; final reflection behavior is documented under the surviving fixes, not as a separate feature. |
| [#4562](https://github.com/BoundaryML/baml/pull/4562) | Yes | Exclude | Test snapshot rendering only; verified from the merged diff. |
| [#4564](https://github.com/BoundaryML/baml/pull/4564) | Yes | Exclude | Release verification checks and C# smoke-call adjustment; the shipped musl fix is #4502. |
| [#4558](https://github.com/BoundaryML/baml/pull/4558) | No | Exclude | CI runner cache access only. |
| [#4565](https://github.com/BoundaryML/baml/pull/4565) | Yes | Keep | C25 |
| [#4569](https://github.com/BoundaryML/baml/pull/4569) | Yes | Exclude | Removal of an already obsolete protocol and debug-only verifier; no additional user effect established. |
| [#4566](https://github.com/BoundaryML/baml/pull/4566) | Yes | Keep | C52 |
| [#4135](https://github.com/BoundaryML/baml/pull/4135) | Yes | Keep | C17 C53 |
| [#4572](https://github.com/BoundaryML/baml/pull/4572) | Yes | Exclude | Internal test compilation acceleration only. |
| [#4575](https://github.com/BoundaryML/baml/pull/4575) | Yes | Exclude | C# verification fixture refactor only. |
| [#4580](https://github.com/BoundaryML/baml/pull/4580) | Yes | Keep | C22 C23 |
| [#4570](https://github.com/BoundaryML/baml/pull/4570) | Yes | Keep | C04 C20 |
| [#4563](https://github.com/BoundaryML/baml/pull/4563) | Yes | Keep | C01 |
| [#4578](https://github.com/BoundaryML/baml/pull/4578) | Yes | Keep | C01 |
| [#4573](https://github.com/BoundaryML/baml/pull/4573) | Yes | Keep | C54 |
| [#4567](https://github.com/BoundaryML/baml/pull/4567) | Yes | Keep | C27 |
| [#4582](https://github.com/BoundaryML/baml/pull/4582) | Yes | Exclude | C# CI cache and test scheduling only. |
| [#4581](https://github.com/BoundaryML/baml/pull/4581) | Yes | Keep | C55 |
| [#4597](https://github.com/BoundaryML/baml/pull/4597) | No | Exclude | Website navigation edit independent of the language release. |
| [#4568](https://github.com/BoundaryML/baml/pull/4568) | Yes | Keep | C56 |
| [#4571](https://github.com/BoundaryML/baml/pull/4571) | Yes | Keep | C57 |
| [#4577](https://github.com/BoundaryML/baml/pull/4577) | Yes | Keep | C58 |
| [#4574](https://github.com/BoundaryML/baml/pull/4574) | Yes | Keep | C11 |
| [#4583](https://github.com/BoundaryML/baml/pull/4583) | Yes | Keep | C43 C59 |
| [#4601](https://github.com/BoundaryML/baml/pull/4601) | Yes | Keep | C24 |
| [#4600](https://github.com/BoundaryML/baml/pull/4600) | Yes | Keep | C60 |
| [#4579](https://github.com/BoundaryML/baml/pull/4579) | No | Exclude | Internal feedback triage tooling. |
| [#4603](https://github.com/BoundaryML/baml/pull/4603) | Yes | Exclude | Compiler type representation cleanup; no separate user effect established. |
| [#4605](https://github.com/BoundaryML/baml/pull/4605) | No | Exclude | Website onboarding copy independent of the language release. |
| [#4607](https://github.com/BoundaryML/baml/pull/4607) | No | Exclude | Website deployment configuration independent of the language release. |
| [#4602](https://github.com/BoundaryML/baml/pull/4602) | Yes | Keep | C26 |
| [#4604](https://github.com/BoundaryML/baml/pull/4604) | Yes | Keep | C05 C06 C19 C20 C21 C61 |
| [#4606](https://github.com/BoundaryML/baml/pull/4606) | Yes | Keep | C18 C68 |
| [#4599](https://github.com/BoundaryML/baml/pull/4599) | Yes | Keep | C14 |
| [#4609](https://github.com/BoundaryML/baml/pull/4609) | Yes | Keep | C67 |
| [#4613](https://github.com/BoundaryML/baml/pull/4613) | No | Exclude | Internal metrics and Slack reporting. |
| [#4614](https://github.com/BoundaryML/baml/pull/4614) | No | Exclude | Internal metrics screenshot readiness. |
| [#4615](https://github.com/BoundaryML/baml/pull/4615) | No | Exclude | Internal metrics screenshot readiness. |
| [#4612](https://github.com/BoundaryML/baml/pull/4612) | Yes | Keep | C62 |
| [#4617](https://github.com/BoundaryML/baml/pull/4617) | No | Exclude | Internal metrics Slack destination. |
| [#4618](https://github.com/BoundaryML/baml/pull/4618) | No | Exclude | Internal metrics report link. |
| [#4608](https://github.com/BoundaryML/baml/pull/4608) | No | Exclude | Internal issue triage tooling. |
| [#4619](https://github.com/BoundaryML/baml/pull/4619) | Yes | Keep | C63 |
| [#4621](https://github.com/BoundaryML/baml/pull/4621) | Yes | Keep | C64 |
| [#4626](https://github.com/BoundaryML/baml/pull/4626) | Yes | Exclude | Test scheduling determinism; production LSP unchanged. |
| [#4593](https://github.com/BoundaryML/baml/pull/4593) | Yes | Keep | C65 |
| [#4628](https://github.com/BoundaryML/baml/pull/4628) | Yes | Exclude | The historical changelog itself; comparison target, not a product change. |
| [#4629](https://github.com/BoundaryML/baml/pull/4629) | Yes | Exclude | Version synchronization for this release. |
