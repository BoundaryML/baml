import { rmSync } from "node:fs";

// wasm-pack writes a catch-all .gitignore into its output directory. npm/pnpm
// packing honors nested ignore files, which would otherwise strip the actual
// WASM core from @boundaryml/baml-bridge-web's `dist` package.
rmSync(new URL("../dist/wasm/.gitignore", import.meta.url), { force: true });
rmSync(new URL("../dist/workerd-wasm/.gitignore", import.meta.url), { force: true });
