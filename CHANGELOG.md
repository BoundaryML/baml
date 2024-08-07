# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

## [0.53.0](https://github.com/boundaryml/baml/compare/0.52.1..0.53.0) - 2024-08-05

### Bug Fixes

- make image[] render correctly in prompts (#855) - ([4a17dce](https://github.com/boundaryml/baml/commit/4a17dce43c05efd5f4ea304f2609fe140de1dd8c)) - Samuel Lijin

### Features

- **(ruby)** implement dynamic types, dynamic clients, images, and audio (#842) - ([4a21eed](https://github.com/boundaryml/baml/commit/4a21eed668f32b042fba61f24c9efb8b3794a420)) - Samuel Lijin
- Codelenses for test cases (#812) - ([7cd8794](https://github.com/boundaryml/baml/commit/7cd87942bf50a72de0ad46154f164fb2c174f25b)) - Anish Palakurthi

### Issue

- removed vertex auth token printing (#846) - ([b839316](https://github.com/boundaryml/baml/commit/b83931665a2c3b840eb6c6d31cf3d01c7926e52e)) - Anish Palakurthi
- Fix google type deserialization issue - ([a55b9a1](https://github.com/boundaryml/baml/commit/a55b9a106176ed1ce34bb63397610c2640b37f16)) - Aaron Villalpando

### Miscellaneous Chores

- clean up release stuff (#836) - ([eed41b7](https://github.com/boundaryml/baml/commit/eed41b7474417d2e65b2c5d742234cc20fc5644e)) - Samuel Lijin
- Add bfcl results to readme, fix links icons (#856) - ([5ef7f3d](https://github.com/boundaryml/baml/commit/5ef7f3db99d8d23ff97f1e8372ee71ab7aa127aa)) - aaronvg
- Fix prompt fiddle and playground styles, add more logging, and add stop-reason to playground (#858) - ([38e3153](https://github.com/boundaryml/baml/commit/38e3153843a17ae1e87ae9879ab4374b083d77d0)) - aaronvg
- Bump version to 0.53.0 - ([fd16839](https://github.com/boundaryml/baml/commit/fd16839a2c0b9d92bd5bdcb57f950e22d0a29959)) - Aaron Villalpando

## [0.52.1](https://github.com/boundaryml/baml/compare/0.52.0..0.52.1) - 2024-07-24

### Bug Fixes

- build python x86_64-linux with an older glibc (#834) - ([db12540](https://github.com/boundaryml/baml/commit/db12540a92abf055e286c60864299f53c246b62a)) - Samuel Lijin

## [0.52.0](https://github.com/boundaryml/baml/compare/0.51.3..0.52.0) - 2024-07-24

### Features

- Add official support for ruby (#823) - ([e81cc79](https://github.com/boundaryml/baml/commit/e81cc79498809a79f427864704b140967a41277a)) - Samuel Lijin

### Bug Fixes

- Fix ClientRegistry for Typescript code-gen (#828) - ([b69921f](https://github.com/boundaryml/baml/commit/b69921f45df0182072b09ab28fe6231ccfaa5767)) - hellovai

## [0.51.2](https://github.com/boundaryml/baml/compare/0.51.1..0.51.2) - 2024-07-24

### Features

- Add support for unions / maps / null in TypeBuilder. (#820) - ([8d9e92d](https://github.com/boundaryml/baml/commit/8d9e92d3050a67edbec5ee6056397becbcdb754b)) - hellovai

### Bug Fixes

- [Playground] Add a feedback button (#818) - ([f749f2b](https://github.com/boundaryml/baml/commit/f749f2b19b247de2f050beccd1fe8e50b7625757)) - Samuel Lijin

### Documentation

- Improvements across docs (#807) - ([bc0c176](https://github.com/boundaryml/baml/commit/bc0c1761699ee2485a0a8ee61cf4fda6b579f974)) - Anish Palakurthi

## [0.51.1](https://github.com/boundaryml/baml/compare/0.51.0..0.51.1) - 2024-07-21

### Features

- Add a feedback button to VSCode Extension (#811) - ([f371912](https://github.com/boundaryml/baml/commit/f3719127174d8f998579747f14fae8675dafba4c)) - Samuel Lijin

### Bug

- Allow default_client_mode in the generator #813 (#815) - ([6df7fca](https://github.com/boundaryml/baml/commit/6df7fcabc1eb55b08a50741f2346440f631abd63)) - hellovai

## [0.51.0](https://github.com/boundaryml/baml/compare/0.50.0..0.51.0) - 2024-07-19

### Bug Fixes

- Improve BAML Parser for numbers and single-key objects (#785) - ([c5af7b0](https://github.com/boundaryml/baml/commit/c5af7b0d0e881c3046171ca17f317d820e8882e3)) - hellovai
- Add docs for VLLM (#792) - ([79e8773](https://github.com/boundaryml/baml/commit/79e8773e38da524795dda606b9fae09a274118e1)) - hellovai
- LLVM install and rebuild script (#794) - ([9ee66ed](https://github.com/boundaryml/baml/commit/9ee66ed2dd14bc0ee12a788f41eae64377e7f2b0)) - Anish Palakurthi
- Prevent version mismatches when generating baml_client (#791) - ([d793603](https://github.com/boundaryml/baml/commit/d7936036e6afa4a0e738242cfb3feaa9e15b3657)) - aaronvg
- fiddle build fix (#800) - ([d304203](https://github.com/boundaryml/baml/commit/d304203241726ac0ba8781db7ac5693339189eb4)) - aaronvg
- Dont drop extra fields in dynamic classes when passing them as inputs to a function (#802) - ([4264c9b](https://github.com/boundaryml/baml/commit/4264c9b143edda0239af197d110357b1969bf12c)) - aaronvg
- Adding support for a sync client for Python + Typescript (#803) - ([62085e7](https://github.com/boundaryml/baml/commit/62085e79d4d86f580ce189bc60f36bd1414893c4)) - hellovai
- Fix WASM-related issues introduced in #803 (#804) - ([0a950e0](https://github.com/boundaryml/baml/commit/0a950e084748837ee2e269504d22dba66f339ca4)) - hellovai
- Adding various fixes (#806) - ([e8c1a61](https://github.com/boundaryml/baml/commit/e8c1a61a96051160566b6458dac5c89d5ddfb86e)) - hellovai

### Features

- implement maps in BAML (#797) - ([97d7e62](https://github.com/boundaryml/baml/commit/97d7e6223c68e9c338fe7110554f1f26b966f7e3)) - Samuel Lijin
- Support Vertex AI (Google Cloud SDK) (#790) - ([d98ee81](https://github.com/boundaryml/baml/commit/d98ee81a9440de0aaa6de05b33b8d3f709003a00)) - Anish Palakurthi
- Add copy buttons to test results in playground (#799) - ([b5eee3d](https://github.com/boundaryml/baml/commit/b5eee3d15a1be4373e25cc8ef1cf6e70d5dd39c9)) - aaronvg

### Miscellaneous Chores

- in fern config, defer to installed version (#789) - ([479f1b2](https://github.com/boundaryml/baml/commit/479f1b2b0b52faf47bc529e4c06c533a9467269a)) - fern
- publish docs on every push to the default branch (#796) - ([180824a](https://github.com/boundaryml/baml/commit/180824a3857a32eae679e4df5704abba3aa6246c)) - Samuel Lijin
- 🌿 introducing fern docs (#779) - ([46f06a9](https://github.com/boundaryml/baml/commit/46f06a95a1e262e62476768b812b372b696da1be)) - fern
- Add test for dynamic list input (#798) - ([7528d6a](https://github.com/boundaryml/baml/commit/7528d6ae10427c1304e356cf5b3c664e4fb2b1b1)) - aaronvg

## [0.50.0](https://github.com/boundaryml/baml/compare/0.49.0..0.50.0) - 2024-07-11

### Bug Fixes

- [Playground] Environment variable button is now visible on all themes (#762) - ([adc4da1](https://github.com/boundaryml/baml/commit/adc4da1fa36cc9c30ea36e25de1a6cefcce0bc97)) - aaronvg
- [Playground] Fix to cURL rendering and mime_type overriding (#763) - ([67f9c6a](https://github.com/boundaryml/baml/commit/67f9c6add5ea8bbbd5ee82c28476fe0ebbefe344)) - Anish Palakurthi

### Features

- [Runtime] Add support for clients that change at runtime using ClientRegistry (#683) - ([c0fb454](https://github.com/boundaryml/baml/commit/c0fb4540d9193194fcafd7fcef71468442d9e6fa)) - hellovai
  https://docs.boundaryml.com/docs/calling-baml/client-registry

### Documentation

- Add more documentation for TypeBuilder (#767) - ([85dc8ab](https://github.com/boundaryml/baml/commit/85dc8ab41e0df3267249a1efc4a95f010e52cc73)) - Samuel Lijin

## [0.49.0](https://github.com/boundaryml/baml/compare/0.46.0..0.49.0) - 2024-07-08

### Bug Fixes

- Fixed Azure / Ollama clients. Removing stream_options from azure and ollama clients (#760) - ([30bf88f](https://github.com/boundaryml/baml/commit/30bf88f65c8583ab02db6a7b7db40c1e9f3b05b6)) - hellovai

### Features

- Add support for arm64-linux (#751) - ([adb8ee3](https://github.com/boundaryml/baml/commit/adb8ee3097fd386370f75b3ba179d18b952e9678)) - Samuel Lijin

## [0.48.0](https://github.com/boundaryml/baml/compare/0.47.0..0.48.0) - 2024-07-04

### Bug Fixes

- Fix env variables dialoge on VSCode (#750)
- Playground selects correct function after loading (#757) - ([09963a0](https://github.com/boundaryml/baml/commit/09963a02e581da9eb8f7bafd3ba812058c97f672)) - aaronvg

### Miscellaneous Chores

- Better error messages on logging failures to Boundary Studio (#754) - ([49c768f](https://github.com/boundaryml/baml/commit/49c768fbe8eb8023cba28b8dc68c2553d8b2318a)) - aaronvg

## [0.47.0](https://github.com/boundaryml/baml/compare/0.46.0..0.47.0) - 2024-07-03

### Bug Fixes

- make settings dialog work in vscode again (#750) ([c94e355](https://github.com/boundaryml/baml/commit/c94e35551872f65404136b60f800fb1688902c11)) - aaronvg
- restore releases on arm64-linux (#751) - ([adb8ee3](https://github.com/boundaryml/baml/commit/adb8ee3097fd386370f75b3ba179d18b952e9678)) - Samuel Lijin

## [0.46.0](https://github.com/boundaryml/baml/compare/0.45.0..0.46.0) - 2024-07-03

### Bug Fixes

- Fixed tracing issues for Boundary Studio (#740) - ([77a4db7](https://github.com/boundaryml/baml/commit/77a4db7ef4b939636472ad4975d74e9d1a577cbf)) - Samuel Lijin
- Fixed flush() to be more reliable (#744) - ([9dd5fda](https://github.com/boundaryml/baml/commit/9dd5fdad5c2897b49a5a536df2e9ef775857a39d)) - Samuel Lijin
- Remove error when user passes in extra fields in a class (#746) - ([2755b43](https://github.com/boundaryml/baml/commit/2755b43257f9405ae66a30982d9711fc3f2c0854)) - aaronvg

### Features

- Add support for base_url for the google-ai provider (#747) - ([005b1d9](https://github.com/boundaryml/baml/commit/005b1d93b7f7d2aa12a1487911766cccd9c25e98)) - hellovai
- Playground UX improvements (#742) - ([5cb56fd](https://github.com/boundaryml/baml/commit/5cb56fdc39496f0aedacd79766c0e93cb0e401b8)) - hellovai
- Prompt Fiddle now auto-switches functions when to change files (#745)

### Documentation

- Added a large example project on promptfiddle.com (#741) - ([f80da1e](https://github.com/boundaryml/baml/commit/f80da1e1dd11f0457b5789bc9ce6923a8ed88b51)) - aaronvg
- Mark ruby as in beta (#743) - ([901109d](https://github.com/boundaryml/baml/commit/901109dbb327e6e3e1b65fda37100fcd45f97e07)) - Samuel Lijin

## [0.45.0](https://github.com/boundaryml/baml/compare/0.44.0..0.45.0) - 2024-06-29

### Bug Fixes

- Fixed streaming in Python Client which didn't show result until later (#726) - ([e4f2daa](https://github.com/boundaryml/baml/commit/e4f2daa9e85bb1711d112fb0c87c0d769be0bb2d)) - Anish Palakurthi
- Improve playground stability on first load (#732) - ([2ac7b32](https://github.com/boundaryml/baml/commit/2ac7b328e89400cba0d9eb4f6d09c6a03feb71a5)) - Anish Palakurthi
- Add improved static analysis for jinja (#734) - ([423faa1](https://github.com/boundaryml/baml/commit/423faa1af5a594b7f78f7bb5620e3146a8989da5)) - hellovai

### Documentation

- Docs for Dynamic Types (#722) [https://docs.boundaryml.com/docs/calling-baml/dynamic-types](https://docs.boundaryml.com/docs/calling-baml/dynamic-types)

### Features

- Show raw cURL request in Playground (#723) - ([57928e1](https://github.com/boundaryml/baml/commit/57928e178549cb3e5118ce374aab5d0fbad7038b)) - Anish Palakurthi
- Support bedrock as a provider (#725) - ([c64c665](https://github.com/boundaryml/baml/commit/c64c66522a1d496493a30f593103209acd201364)) - Samuel Lijin

## [0.44.0](https://github.com/boundaryml/baml/compare/0.43.0..0.44.0) - 2024-06-26

### Bug Fixes

- Fix typebuilder for random enums (#721)

## [0.43.0](https://github.com/boundaryml/baml/compare/0.42.0..0.43.0) - 2024-06-26

### Bug Fixes

- fix pnpm lockfile issue (#720)

## [0.42.0](https://github.com/boundaryml/baml/compare/0.41.0..0.42.0) - 2024-06-26

### Bug Fixes

- correctly propagate LICENSE to baml-py (#695) - ([3fda880](https://github.com/boundaryml/baml/commit/3fda880bf39b32191b425ae75e8b491d10884cf6)) - Samuel Lijin

### Miscellaneous Chores

- update jsonish readme (#685) - ([b19f04a](https://github.com/boundaryml/baml/commit/b19f04a059ba18d54544cb278b6990b95170d3f3)) - Samuel Lijin

### Vscode

- add link to tracing, show token counts (#703) - ([64aa18a](https://github.com/boundaryml/baml/commit/64aa18a9cc34071655141c8f6e2ad04ac90e7be1)) - Samuel Lijin

## [0.41.0] - 2024-06-20

### Bug Fixes

- rollback git lfs, images broken in docs rn (#534) - ([6945506](https://github.com/boundaryml/baml/commit/694550664fa45b5f76987e2663c9d7e7a9a6a2d2)) - Samuel Lijin
- search for markdown blocks correctly (#641) - ([6b8abf1](https://github.com/boundaryml/baml/commit/6b8abf1ccf55bbe7c3bc1046c78081126e01f134)) - Samuel Lijin
- restore one-workspace-per-folder (#656) - ([a464bde](https://github.com/boundaryml/baml/commit/a464bde566199ace45285a78a7f542cd7217fb65)) - Samuel Lijin
- ruby generator should be ruby/sorbet (#661) - ([0019f39](https://github.com/boundaryml/baml/commit/0019f3951b8fe2b49e62eb11d869516b8088e9cb)) - Samuel Lijin
- ruby compile error snuck in (#663) - ([0cb2583](https://github.com/boundaryml/baml/commit/0cb25831788eb8b3eb0a38383917f6d1ffb5633a)) - Samuel Lijin

### Documentation

- add typescript examples (#477) - ([532481c](https://github.com/boundaryml/baml/commit/532481c3df4063b37a8834a5fe2bbce3bb37d2f5)) - Samuel Lijin
- add titles to code blocks for all CodeGroup elems (#483) - ([76c6b68](https://github.com/boundaryml/baml/commit/76c6b68b27ee37972fa226be0b4dfe31f7b4b5ec)) - Samuel Lijin
- add docs for round-robin clients (#500) - ([221f902](https://github.com/boundaryml/baml/commit/221f9020d850e6d24fe2fd8a684081726a0659af)) - Samuel Lijin
- add ruby example (#689) - ([16e187f](https://github.com/boundaryml/baml/commit/16e187f6698a1cc86a37eedf2447648d810370ad)) - Samuel Lijin

### Features

- implement `baml version --check --output json` (#444) - ([5f076ac](https://github.com/boundaryml/baml/commit/5f076ace1f92dc2141b231c9e62f4dc23f7fef18)) - Samuel Lijin
- show update prompts in vscode (#451) - ([b66da3e](https://github.com/boundaryml/baml/commit/b66da3ee355fcd6a8677d834ecb05af44cbf8f20)) - Samuel Lijin
- add tests to check that baml version --check works (#454) - ([be1499d](https://github.com/boundaryml/baml/commit/be1499dfa82ff8ab923a16d45290758120d95015)) - Samuel Lijin
- parse typescript versions in version --check (#473) - ([b4b2250](https://github.com/boundaryml/baml/commit/b4b2250c37b900db899256159bbfc3aa2ec819cb)) - Samuel Lijin
- implement round robin client strategies (#494) - ([599fcdd](https://github.com/boundaryml/baml/commit/599fcdd2a45c5b1e935f36769784ca944566b88c)) - Samuel Lijin
- add integ-tests support to build (#542) - ([f59cf2e](https://github.com/boundaryml/baml/commit/f59cf2e1a9ec7edbe174f4bc7ff9391f2cff3208)) - Samuel Lijin
- make ruby work again (#650) - ([6472bec](https://github.com/boundaryml/baml/commit/6472bec231b581076ee7edefaab2e7979b2bf336)) - Samuel Lijin
- Add RB2B tracking script (#682) - ([54547a3](https://github.com/boundaryml/baml/commit/54547a34d40cd40a43767919dbc9faa68a82faea)) - hellovai

### Miscellaneous Chores

- add nodemon config to typescript/ (#435) - ([231b396](https://github.com/boundaryml/baml/commit/231b3967bc947c4651156bc55fd66552782824c9)) - Samuel Lijin
- finish gloo to BoundaryML renames (#452) - ([88a7fda](https://github.com/boundaryml/baml/commit/88a7fdacc826e78ef21c6b24745ee469d9d02e6a)) - Samuel Lijin
- set up lfs (#511) - ([3a43143](https://github.com/boundaryml/baml/commit/3a431431e8e38dfc68763f15ccdcd1d131f23984)) - Samuel Lijin
- add internal build tooling for sam (#512) - ([9ebacca](https://github.com/boundaryml/baml/commit/9ebaccaa542760cb96382ae2a91d780f1ade613b)) - Samuel Lijin
- delete clients dir, this is now dead code (#652) - ([ec2627f](https://github.com/boundaryml/baml/commit/ec2627f59c7fe9edfff46fcdb65f9b9f0e2e072c)) - Samuel Lijin
- consolidate vscode workspace, bump a bunch of deps (#654) - ([82bf6ab](https://github.com/boundaryml/baml/commit/82bf6ab1ad839f84782a7ef0441f21124c368757)) - Samuel Lijin
- Add RB2B tracking script to propmt fiddle (#681) - ([4cf806b](https://github.com/boundaryml/baml/commit/4cf806bba26563fd8b6ddbd68296ab8bdfac21c4)) - hellovai
- Adding better release script (#688) - ([5bec282](https://github.com/boundaryml/baml/commit/5bec282d39d2250b39ef4aba5d6bba9830a35988)) - hellovai

### [AUTO

- patch] Version bump for nightly release [NIGHTLY:cli] [NIGHTLY:vscode_ext] [NIGHTLY:client-python] - ([d05a22c](https://github.com/boundaryml/baml/commit/d05a22ca4135887738adbce638193d71abca42ec)) - GitHub Action

### Build

- fix baml-core-ffi script (#521) - ([b1b7f4a](https://github.com/boundaryml/baml/commit/b1b7f4af0991ef6453f888f27930f3faaae337f5)) - Samuel Lijin
- fix engine/ (#522) - ([154f646](https://github.com/boundaryml/baml/commit/154f6468ec0aa6de1b033ee1cbc76e60acc363ea)) - Samuel Lijin

### Integ-tests

- add ruby test - ([c0bc101](https://github.com/boundaryml/baml/commit/c0bc10126ea32d099f1398f2c5faa08b111554ba)) - Sam Lijin

### Readme

- add function calling, collapse the table (#505) - ([2f9024c](https://github.com/boundaryml/baml/commit/2f9024c28ba438267de37ac43c6570a2f0398b5a)) - Samuel Lijin

### Release

- bump versions for everything (#662) - ([c0254ae](https://github.com/boundaryml/baml/commit/c0254ae680365854c51c7a4e58ea68d1901ea033)) - Samuel Lijin

### Vscode

- check for updates on the hour (#434) - ([c70a3b3](https://github.com/boundaryml/baml/commit/c70a3b373cb2346a0df9a1eba0ebacb74d59b53e)) - Samuel Lijin

<!-- generated by git-cliff -->
