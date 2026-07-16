import { rmSync } from "node:fs";

rmSync(new URL("../dist/wasm", import.meta.url), { recursive: true, force: true });
