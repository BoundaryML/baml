# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

## [0.56.1](https://github.com/boundaryml/baml/compare/0.56.0..0.56.1) - 2024-09-21


### Bug Fixes
- Improved parser for unions (#975) - ([b390521](https://github.com/boundaryml/baml/commit/b39052111529f217762b3271846006bec4a604de)) - hellovai
- [syntax] Allow lists to contain trailing comma (#974) - ([9e3dc6c](https://github.com/boundaryml/baml/commit/9e3dc6c90954905a96b599ef28c40094fe48a43e)) - Greg Hale

## [0.56.0](https://github.com/boundaryml/baml/compare/0.55.3..0.56.0) - 2024-09-20

Shout outs to Nico for fixing some internal Rust dependencies, and to Lorenz for correcting our documentation! We really appreciate it :)


### Features

- use better default for openapi/rust client (#958) - ([b74ef15](https://github.com/boundaryml/baml/commit/b74ef15fd4dc09ecc7d1ac8284e7f22cd6d5864c)) - Samuel Lijin

### Bug Fixes

- push optional-list and optional-map validation to post-parse (#959) - ([c0480d5](https://github.com/boundaryml/baml/commit/c0480d5cfd46ce979e957223dc7b5fa744778552)) - Samuel Lijin
- improve OpenAPI instructions for windows/java (#962) - ([6010efb](https://github.com/boundaryml/baml/commit/6010efbb7990fda966640c3af267de41362d3fa4)) - Samuel Lijin
- assorted fixes: unquoted strings, openai-generic add api_key for bearer auth, support escape characters in quoted strings (#965) - ([847f3a9](https://github.com/boundaryml/baml/commit/847f3a9bb0f00303eae7e410663efc63e54c38b6)) - hellovai
- serde-serialize can cause a package dependency cycle (#967) - ([109ae09](https://github.com/boundaryml/baml/commit/109ae0914852f2ee4a771d27103e4e46ad672647)) - Nico
- make anthropic work in fiddle/vscode (#970) - ([32eccae](https://github.com/boundaryml/baml/commit/32eccae44b27c3fec5fbc3270b6657819d75a426)) - Samuel Lijin
- make dynamic enums work as outputs in Ruby (#972) - ([7530402](https://github.com/boundaryml/baml/commit/7530402f0dc063f10f57cf7aa7f06790574de705)) - Samuel Lijin

### Documentation

- suggest correct python init command in vscode readme (#954) - ([e99c5dd](https://github.com/boundaryml/baml/commit/e99c5dd1903078d08aef451e4addc6110d7ca279)) - Samuel Lijin
- add more vscode debugging instructions (#955) - ([342b657](https://github.com/boundaryml/baml/commit/342b657da69441306fa7711d7d14893cf8036f84)) - Samuel Lijin
- NextJS hook needs to be bound to the correct context (#957) - ([ee80451](https://github.com/boundaryml/baml/commit/ee80451de85063b37e658ba58571c791e8514273)) - aaronvg
- update nextjs hooks and docs (#952) - ([01cf855](https://github.com/boundaryml/baml/commit/01cf855500159066fdcd162dc2e2087768d5ba28)) - aaronvg
- Fix some documentation typos (#966) - ([5193cd7](https://github.com/boundaryml/baml/commit/5193cd70686173c863af5ce40fd6bb3792406951)) - Greg Hale
- Keywords AI router (#953) - ([1c6f975](https://github.com/boundaryml/baml/commit/1c6f975d8cc793841745da0db82ee1e2f1908e56)) - aaronvg
- Fix `post_generate` comment (#968) - ([919c79f](https://github.com/boundaryml/baml/commit/919c79fa8cd85a96e6559055b2bb436d925dcb2a)) - lorenzoh

### Bug Fixes

- show actionable errors for string[]? and map<...>? type validation (#946) - ([48879c0](https://github.com/boundaryml/baml/commit/48879c0744f79b482ef0d2b0624464053558ada4)) - Samuel Lijin

### Documentation

- add reference docs about env vars (#945) - ([dd43bc5](https://github.com/boundaryml/baml/commit/dd43bc59087e809e09ca7d3caf628e179a28fc3e)) - Samuel Lijin


## [0.55.2](https://github.com/boundaryml/baml/compare/0.55.1..0.55.2) - 2024-09-11

### Bug Fixes

- use correct locking strategy inside baml-cli serve (#943) - ([fcb694d](https://github.com/boundaryml/baml/commit/fcb694d033317d8538cc7b2c61aaa94f772778db)) - Samuel Lijin

### Features

- allow using DANGER_ACCEPT_INVALID_CERTS to disable https verification (#901) - ([8873fe7](https://github.com/boundaryml/baml/commit/8873fe7577bc879cf0d550063252c4532dcdfced)) - Samuel Lijin

## [0.55.1](https://github.com/boundaryml/baml/compare/0.55.0..0.55.1) - 2024-09-10

### Bug Fixes

- in generated TS code, put eslint-disable before ts-nocheck - ([16d04c6](https://github.com/BoundaryML/baml/commit/16d04c6e360eefca10b4e0d008b03c34de279491)) - Sam Lijin
- baml-cli in python works again - ([b57ca0f](https://github.com/boundaryml/baml/commit/b57ca0f529c80f59b79b19132a8f1339a6b7bfe2)) - Sam Lijin

### Documentation

- update java install instructions (#933) - ([b497003](https://github.com/boundaryml/baml/commit/b49700356f2f69c4acbdc953a66a95224656ffaf)) - Samuel Lijin

### Miscellaneous Chores

- add version headers to the openapi docs (#931) - ([21545f2](https://github.com/boundaryml/baml/commit/21545f2a4d9b3987134d98ac720705dde2045290)) - Samuel Lijin

## [0.55.0](https://github.com/boundaryml/baml/compare/0.54.2..0.55.0) - 2024-09-09

With this release, we're announcing support for BAML in all languages: we now
allow you to call your functions over an HTTP interface, and will generate an
OpenAPI specification for your BAML functions, so you can now generate a client
in any language of your choice, be it Golang, Java, PHP, Ruby, Rust, or any of
the other languages which OpenAPI supports.

Start here to learn more: https://docs.boundaryml.com/docs/get-started/quickstart/openapi

### Features

- implement BAML-over-HTTP (#908) - ([484fa93](https://github.com/boundaryml/baml/commit/484fa93a5a4b4677f531e6ef03bb88d144925c12)) - Samuel Lijin
- Add anonymous telemetry about playground actions (#925) - ([6f58c9e](https://github.com/boundaryml/baml/commit/6f58c9e3e464a8e774771706c2b0d76adb9e6cda)) - hellovai

## [0.54.2](https://github.com/boundaryml/baml/compare/0.54.1..0.54.2) - 2024-09-05

### Features
- Add a setting to disable restarting TS server in VSCode (#920) - ([628f236](https://github.com/boundaryml/baml/commit/628f2360c415fa8a7b0cd90d7249733ff06acaa9)) - aaronvg
- Add prompt prefix for map types in ctx.output_format and add more type validation for map params (#919) - ([4d304c5](https://github.com/boundaryml/baml/commit/4d304c583b9188c1963a34e2a153baaf003e36ac)) - hellovai

### Bug fixes
- Fix glibC issues for python linux-x86_64 (#922) - ([9161bec](https://github.com/boundaryml/baml/commit/9161becccf626f8d13a15626481720f29e0f992c)) - Samuel Lijin


### Documentation
- Add nextjs hooks (#921) - ([fe14f5a](https://github.com/boundaryml/baml/commit/fe14f5a4ef95c9ccda916ff80ce852d3855554a3)) - aaronvg


## [0.54.1](https://github.com/boundaryml/baml/compare/0.54.0..0.54.1) - 2024-09-03

### BREAKING CHANGE

- Fix escape characters in quoted strings (#905) - ([9ba6eb8](https://github.com/boundaryml/baml/commit/9ba6eb834e0145f4c57e582b63730d3d0ac9b2e9)) - hellovai

Prior `"\n"` was interpreted as `"\\n"` in quoted strings. This has been fixed to interpret `"\n"` as newline characters and true for other escape characters.


### Documentation

- updated dead vs-code-extension link (#914) - ([b12f164](https://github.com/boundaryml/baml/commit/b12f1649cf5bfd0d457c5d6d117fd3a21ba5dc6b)) - Christian Warmuth
- Update docs for setting env vars (#904) - ([ec1ca94](https://github.com/boundaryml/baml/commit/ec1ca94c91af2a51b4190a0bad0e0bc1c052f2a3)) - hellovai
- Add docs for LMStudio (#906) - ([ea4c187](https://github.com/boundaryml/baml/commit/ea4c18782de1f713e8d69d473f9e1818c97024c6)) - hellovai
- Fix docs for anthropic (#910) - ([aba2764](https://github.com/boundaryml/baml/commit/aba2764e5b04820d00b08bf52bda603ee27631f1)) - hellovai
- Update discord links on docs (#911) - ([927357d](https://github.com/boundaryml/baml/commit/927357dd64b36c25513352ed4968ebc62dad6132)) - hellovai

### Features

- BAML_LOG will truncate messages to 1000 characters (modify using env var BOUNDARY_MAX_LOG_CHUNK_SIZE) (#907) - ([d266e5c](https://github.com/boundaryml/baml/commit/d266e5c4157f3b28d2f6454a7ea265dda7296bb2)) - hellovai

### Bug Fixes
- Improve parsing parsing when there are initial closing `]` or `}` (#903) - ([46b0cde](https://github.com/boundaryml/baml/commit/46b0cdeffb15bbab20a43728f52ad2a05623e6f7)) - hellovai
- Update build script for ruby to build all platforms (#915) - ([df2f51e](https://github.com/boundaryml/baml/commit/df2f51e52615451b3643cc124e7262f11965f3ef)) - hellovai
- Add unit-test for openai-generic provider and ensure it compiles (#916) - ([fde7c50](https://github.com/boundaryml/baml/commit/fde7c50c939c505906417596d16c7c4607173339)) - hellovai


## [0.54.0](https://github.com/boundaryml/baml/compare/0.53.1..0.54.0) - 2024-08-27

### BREAKING CHANGE

- Update Default Gemini Base URL to v1beta (#891) - ([a5d8c58](https://github.com/boundaryml/baml/commit/a5d8c588e0fd0b7e186d7c71f1f6171334250629)) - gleed

The default base URL for the Gemini provider has been updated to v1beta. This change is should have no impact on existing users as v1beta is the default version for the Gemini python library, we are mirroring this change in BAML.

### Bug Fixes

- Allow promptfiddle to talk to localhost ollama (#886) - ([5f02b2a](https://github.com/boundaryml/baml/commit/5f02b2ac688ceeb5a34e848a8ff87fd43a6b093a)) - Samuel Lijin
- Update Parser for unions so they handle nested objects better (#900) - ([c5b9a75](https://github.com/boundaryml/baml/commit/c5b9a75ea6da7c45da1999032e2b256bec97d922)) - hellovai

### Documentation

- Add ollama to default prompt fiddle example (#888) - ([49146c0](https://github.com/boundaryml/baml/commit/49146c0e50c88615e4cc97adb595849c23bad8ae)) - Samuel Lijin
- Adding improved docs + unit tests for caching (#895) - ([ff7be44](https://github.com/boundaryml/baml/commit/ff7be4478b706da049085d432b2ec98627b5da1f)) - hellovai

### Features

- Allow local filepaths to be used in tests in BAML files (image and audio) (#871) - ([fa6dc03](https://github.com/boundaryml/baml/commit/fa6dc03fcdd3255dd83e25d0bfb3b0e740991408)) - Samuel Lijin
- Add support for absolute file paths in the file specifier (#881) - ([fcd189e](https://github.com/boundaryml/baml/commit/fcd189ed7eb81712bf3b641eb3dde158fc6a62af)) - hellovai
- Implement shorthand clients (You can now use "openai/gpt-4o" as short for creating a complete client.) (#879) - ([ddd15c9](https://github.com/boundaryml/baml/commit/ddd15c92c3e8d81c24cb7305c9fcbb36b819900f)) - Samuel Lijin
- Add support for arbritrary metadata (e.g. cache_policy for anthropic)  (#893) - ([0d63a70](https://github.com/boundaryml/baml/commit/0d63a70332477761a97783e203c98fd0bf67f151)) - hellovai
- Expose Exceptions to user code: BamlError, BamlInvalidArgumentError, BamlClientError, BamlClientHttpError, BamlValidationError (#770) - ([7da14c4](https://github.com/boundaryml/baml/commit/7da14c480506e9791b3f4ce52ac73836a042d38a)) - hellovai


### Internal
- AST Restructuring (#857) - ([75b51cb](https://github.com/boundaryml/baml/commit/75b51cbf80a0c8ba19ae05b021ef3c94dacb4e30)) - Anish Palakurthi

## [0.53.1](https://github.com/boundaryml/baml/compare/0.53.0..0.53.1) - 2024-08-11

### Bug Fixes

- fix github release not passing params to napi script causing issues in x86_64 (#872)

- ([06b962b](https://github.com/boundaryml/baml/commit/06b962b945f958bf0637d13fec22bd2d59c64c5f)) - aaronvg

### Features

- Add Client orchestration graph in playground (#801) - ([24b5895](https://github.com/boundaryml/baml/commit/24b5895a1f45ac04cba0f19e6da727b5ee766186)) - Anish Palakurthi
- increase range of python FFI support (#870) - ([ec9b66c](https://github.com/boundaryml/baml/commit/ec9b66c31faf97a58c81c264c7fa1b32e0e9f0ae)) - Samuel Lijin

### Misc

- Bump version to 0.53.1 - ([e4301e3](https://github.com/boundaryml/baml/commit/e4301e37835483f51edf1cad6478e46ff67508fc)) - Aaron Villalpando

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
