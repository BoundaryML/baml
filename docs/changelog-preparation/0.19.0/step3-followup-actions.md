# Followup drafts for BAML 0.19.0

These are drafts to post after 0.19.0 and its changelog are published. No messages have been sent. Review the final release SHA before posting. The installer response additionally depends on the compatible wrapper and installer being publicly available, since the wrapper is released independently.

## GitHub PR: [#4751 — float math](https://github.com/BoundaryML/baml/pull/4751)

External contributor: [@ritunjaym](https://github.com/ritunjaym).

> Adds nine methods to `float`.

```text
Thanks @ritunjaym! Your float math methods and finite-range constants are included in BAML 0.19.0: exp, ln, log2, log10, cbrt, signum, max_finite, min_finite, and epsilon. The release notes preserve the accuracy qualifications from your review. The separate signed-zero literal issue #4750 is still outstanding. Release notes: https://boundaryml.com/changelog.
```

## GitHub PR: [#4752 — contributor setup documentation](https://github.com/BoundaryML/baml/pull/4752)

External contributor: [@ritunjaym](https://github.com/ritunjaym). Included in the complete v1 range even though contributor setup is omitted from the public feature list.

> `README-DEV.md` describes Rust as a mise-managed tool pinned to 1.88.0.

```text
Thanks @ritunjaym! Your contributor setup documentation cleanup is included in the source revision for BAML 0.19.0. README-DEV now describes mise, direnv, and rustup without stale Rust version pins or a duplicate tools list. Release notes: https://boundaryml.com/changelog.
```

## GitHub issue: [#4624 — Linux installer bootstrap](https://github.com/BoundaryML/baml/issues/4624)

External reporter: [@BenSpex](https://github.com/BenSpex). Fix: [#4632](https://github.com/BoundaryML/baml/pull/4632).

> The official installer should work on a currently supported stable Debian base.

```text
Thanks @BenSpex! The installer update accompanying BAML 0.19.0 addresses the wrapper bootstrap failure you reported. It selects a compatible GNU wrapper and falls back to musl; the fix was tested on Debian Bookworm and Alpine on x86_64 and ARM64. The wrapper is versioned separately from the language toolchain, so rerun the current installer to pick up the wrapper fix. Release notes: https://boundaryml.com/changelog.
```

## Reviewed reports that should not receive a new fixed notification

| Report | Decision |
| --- | --- |
| [#4750](https://github.com/BoundaryML/baml/issues/4750) / BAMLGH-73 | Still reproducible at the cut: the two zero literals print `"0.0,0.0"`. #4751 adds signum but does not fix literal aliasing. |
| [#4587](https://github.com/BoundaryML/baml/issues/4587) / BAMLGH-61 | The reporter confirmed the unresolved-throws crash fixed before 0.18.0. #4686 covers additional diagnostic/LSP paths. Do not present the older report as newly fixed. |
| [#4468](https://github.com/BoundaryML/baml/issues/4468) | The array/map/loop reproducer already executes correctly with 0.18.0. |
| [#4370](https://github.com/BoundaryML/baml/issues/4370) / BAMLGH-44 | Historical Rust generator report linked as context, not a fix attributed to this range. |
| [#2736](https://github.com/BoundaryML/baml/issues/2736) | Historical v0 Python extension GLIBC issue, not the v1 wrapper bootstrap issue. |
| [#1259](https://github.com/BoundaryML/baml/issues/1259) / BAMLGH-4 | Historical Cloudflare feature request opened by a current member; not an external-user notification. B-1656 is the concrete v1 startup fix. |
| B-1680 | Future `unreflect` redesign, not shipped by this cut. |
| B-1655 | Integration-test lifecycle design question, not addressed by the workerd startup fix. |

## Source-thread gaps and deduplication

The body of #4751 says the float proposal was discussed on Discord but supplies no thread URL. Its PR comments, reviews, inline review comments, linked issue #4750, and BAMLGH-73 contain no Discord link. No verified destination or external-thread quote is available; do not invent a Discord reply target. The PR reply above reaches the identified contributor. Locate the original thread before adding a separate Discord draft.

The two direct Linear source threads in Slack were read in full and contain internal discussion. Two historical source threads attached to B-1123 were also read. One forwards older external reports without identifying an external reply destination; those reports are not evidence of additional fixes in this range. No Slack messages are proposed.

External status is based on GitHub association: MEMBER, OWNER, and COLLABORATOR are treated as internal; bot accounts are excluded. All other human PR authors, commenters, reviewers, and inline reviewers were checked. The only external PR participant found in this range is @ritunjaym, on #4751 and #4752. Linear mirror issues are deduplicated to their GitHub source. These three drafts cover two distinct external users.
