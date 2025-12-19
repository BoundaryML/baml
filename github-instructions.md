Your goal is to make the one-off npm publish workflow defined in .github/workflows/npm-adhoc.yml succeed. Make changes to the workflow, push, and repeat until the latest run of that workflow passes.

# Iteration Loop
Use this iteration loop when testing changes to .github/workflows/build-typescript-release-win-arm64.yaml:
- `git commit -a -m 'message goes here' && git push` will commit all local changes, push them to github, and run the workflow (the workflow has an on-push trigger, so any push will trigger the workflow)
- Grab the latest run id for that branch: `RUN_ID=$(gh run list --workflow "Download Bindings From Run" --limit 1 --json databaseId --jq '.[0].databaseId' --repo boundaryml/baml --ref sam/npm-adhoc2`
- Stream logs until it finishes: `gh run view "$RUN_ID" --repo boundaryml/baml --log --watch`
Notes:
- all `gh` commands need `--repo` and `--ref` to be specified since there's no local git repo to infer from

# Goals
We recently added arm64 windows support to the artifact build process, but our release automation is having issues pushing this to github.

Here are logs that show what the issue is:

```
Run pnpm --filter=@boundaryml/baml publish --access public --no-git-checks
> @boundaryml/baml@0.215.0 prepublishOnly /home/runner/work/baml/baml/engine/language_client_typescript
> napi create-npm-dirs && pnpm artifacts && napi prepublish --no-gh-release
 INFO  @boundaryml/baml -darwin-arm64 created
 INFO  @boundaryml/baml -linux-arm64-gnu created
 INFO  @boundaryml/baml -linux-arm64-musl created
 INFO  @boundaryml/baml -darwin-x64 created
 INFO  @boundaryml/baml -win32-x64-msvc created
 INFO  @boundaryml/baml -linux-x64-gnu created
 INFO  @boundaryml/baml -linux-x64-musl created
> @boundaryml/baml@0.215.0 artifacts /home/runner/work/baml/baml/engine/language_client_typescript
> mkdir -p artifacts && napi artifacts
> @boundaryml/baml@0.215.0 artifacts /home/runner/work/baml/baml/engine/language_client_typescript
> mkdir -p artifacts && napi artifacts
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-aarch64-apple-darwin/baml.darwin-arm64.node]
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-aarch64-pc-windows-msvc/baml.win32-arm64-msvc.node]
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-aarch64-unknown-linux-gnu/baml.linux-arm64-gnu.node]
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-aarch64-unknown-linux-musl/baml.linux-arm64-musl.node]
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-x86_64-apple-darwin/baml.darwin-x64.node]
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-x86_64-pc-windows-msvc/baml.win32-x64-msvc.node]
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-x86_64-unknown-linux-gnu/baml.linux-x64-gnu.node]
 INFO  Read [/home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-x86_64-unknown-linux-musl/baml.linux-x64-musl.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/npm/darwin-arm64/baml.darwin-arm64.node]
Internal Error: No dist dir found for /home/runner/work/baml/baml/engine/language_client_typescript/artifacts/bindings-aarch64-pc-windows-msvc/baml.win32-arm64-msvc.node
    at file:///home/runner/work/baml/baml/node_modules/.pnpm/@napi-rs+cli@3.0.4_@emnapi+runtime@1.6.0_@types+node@20.19.1/node_modules/@napi-rs/cli/dist/cli.js:725:19
    at async Promise.all (index 1)
    at async collectArtifacts (file:///home/runner/work/baml/baml/node_modules/.pnpm/@napi-rs+cli@3.0.4_@emnapi+runtime@1.6.0_@types+node@20.19.1/node_modules/@napi-rs/cli/dist/cli.js:709:2)
    at async ArtifactsCommand.execute (file:///home/runner/work/baml/baml/node_modules/.pnpm/@napi-rs+cli@3.0.4_@emnapi+runtime@1.6.0_@types+node@20.19.1/node_modules/@napi-rs/cli/dist/cli.js:768:3)
    at async ArtifactsCommand.validateAndExecute (file:///home/runner/work/baml/baml/node_modules/.pnpm/clipanion@4.0.0-rc.4_typanion@3.14.0/node_modules/clipanion/lib/advanced/Command.mjs:49:26)
    at async Cli.run (file:///home/runner/work/baml/baml/node_modules/.pnpm/clipanion@4.0.0-rc.4_typanion@3.14.0/node_modules/clipanion/lib/advanced/Cli.mjs:227:24)
    at async Cli.runExit (file:///home/runner/work/baml/baml/node_modules/.pnpm/clipanion@4.0.0-rc.4_typanion@3.14.0/node_modules/clipanion/lib/advanced/Cli.mjs:236:28)
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/npm/darwin-x64/baml.darwin-x64.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/npm/linux-arm64-gnu/baml.linux-arm64-gnu.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/npm/win32-x64-msvc/baml.win32-x64-msvc.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/npm/linux-arm64-musl/baml.linux-arm64-musl.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/baml.darwin-arm64.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/baml.darwin-x64.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/baml.linux-arm64-gnu.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/baml.win32-x64-msvc.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/baml.linux-arm64-musl.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/npm/linux-x64-gnu/baml.linux-x64-gnu.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/npm/linux-x64-musl/baml.linux-x64-musl.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/baml.linux-x64-gnu.node]
 INFO  Write file content to [/home/runner/work/baml/baml/engine/language_client_typescript/baml.linux-x64-musl.node]
 ELIFECYCLE  Command failed with exit code 1.
 ELIFECYCLE  Command failed with exit code 1.
```
