// Shared prompt fragments embedded (as string props) into the MDX prompts.
// Workflow scaffolding; removed by the final cleanup task.
import { ARTIFACT_DIR_REL, tevmRepoPath } from "./constants.ts";

// Function, not const: the TEVM path is resolved from workflow input at render time.
export const NO_TEVM_RULE = () =>
  `HARD RULE: you are FORBIDDEN from reading, listing, grepping, or otherwise accessing ${tevmRepoPath()} (or any other TEVM checkout). Everything you need from TEVM is already captured in the architecture artifact you are given.`;
export const NO_SMITHERS_RULE = `Never modify anything under .smithers/ (you may READ files under ${ARTIFACT_DIR_REL}/). Never touch .git internals directly; use normal git commands only where instructed.`;

// Originally user steering added mid-flight on 2026-08-03; now permanent design
// principles applied from round zero (retro changes 1-3). These are BLOCKING
// requirements for review approval, not suggestions.
export const USER_STEERING = `DESIGN PRINCIPLES (mandatory, blocking):
1. FOLLOW BAML'S OWN REPO STRUCTURE, NOT TEVM'S. BAML would not split this into many small packages. Consolidate the TypeScript surface into the FEWEST packages consistent with how ${""}typescript2/ and baml_language/ are already organized (target: one tooling package that contains the resolver core, the unplugin bridge, and the tsserver plugin as entry points/subpaths, unless BAML's existing conventions clearly demand a split). Merge packages that exist only to mirror TEVM's layout.
2. NO REPEATED CODE. The unplugin layer must be a thin adapter over the shared core — hoist any logic duplicated across bundler targets (or duplicated between unplugin/tsserver paths) into the shared core so each adapter is a few lines of wiring.
3. MINIMAL, JUSTIFIED DIFF. Every added file and abstraction must be highly justified; prefer extending existing BAML modules over introducing new ones. Delete speculative code, unused options, and scaffolding that is not needed by the shipped feature set. Smaller change > architectural completeness.
4. CLEAN-CONSUMER PROOF. At least one e2e must install the PACKED artifacts (npm pack of every published package) into a fresh scratch project and exercise the full feature surface with ZERO internal environment variables or test-only hooks set. If a feature only works because a test sets an internal env var, the feature does not work.
5. DECLARATION/RUNTIME PARITY. Any generator that projects one source into both type declarations and runtime code must have a test asserting the runtime module exports exactly what the declarations claim (names AND value-ness — a declared enum/const must exist at runtime). One generator with two projections is a standing divergence hazard.`;

export const FEATURE_BRIEF = `The feature: bring first-class TypeScript tooling to BAML — TEVM pioneered the TECHNIQUES (direct non-TS imports, virtual modules, language-service projection), but BAML's own repository conventions govern ALL structure, naming, and package layout. TypeScript projects importing BAML get first-class compiler and editor support:
- a core "technology package" that wraps the existing Rust BAML compiler (native and/or wasm bridge) behind a clean TypeScript API: parse/compile .baml sources, resolve project layout, and emit typed artifacts on demand;
- a minimal unplugin bridge (one plugin core serving Vite/Webpack/Rollup/esbuild/Bun) for build-time integration;
- a TypeScript language-service plugin providing virtual modules and generated types for BAML imports, with source maps back to the .baml sources;
- complete editor features across the TS boundary: go-to-definition (from TS usage into the .baml declaration), find-references, rename, diagnostics, completions, and hover;
- caching and HMR so edits to .baml files update types and modules incrementally;
- packaging that fits BAML's existing release conventions;
- integration and e2e tests proving each feature.`;
