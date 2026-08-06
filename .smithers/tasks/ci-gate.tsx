// Task 5b. Deterministic CI gate (retro change 5): run the repo's own mechanical
// hooks before the expensive reviewer ever mounts. Async spawns so long checks
// never block the engine's event loop.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { join } from "node:path";
import { z } from "zod/v4";
import { REPO_ROOT } from "../lib/constants.ts";
import { shGate } from "../lib/helpers.ts";

export const ciGateSchema = z.object({
  ok: z.literal(true),
  checks: z.array(z.string()).min(3),
  notes: z.string().default(""),
});

export function CiGate({ outputs }: { outputs: any }) {
  return (
    <Task
      id="ci-gate"
      output={outputs.bttCiGate}
      needs={{ guard: "implement-guard" }}
      deps={{ guard: outputs.bttImplGuard }}
      timeoutMs={45 * 60_000}
      retries={1}
    >
      {async () => {
        const rust = join(REPO_ROOT, "baml_language");
        const ts = join(REPO_ROOT, "typescript2");
        const checks: string[] = [];
        checks.push(
          await shGate("cargo fmt --check", "cargo", [
            "fmt", "--all", "--", "--check",
            "--config", "imports_granularity=Crate",
            "--config", "group_imports=StdExternalCrate",
          ], rust),
        );
        checks.push(await shGate("cargo-stow workspace invariants", "cargo", ["run", "-p", "cargo-stow", "--", "stow"], rust));
        checks.push(
          await shGate("cargo clippy -D warnings", "cargo", [
            "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings",
          ], rust),
        );
        checks.push(await shGate("tooling package typecheck", "pnpm", ["--filter", "@boundaryml/baml-tooling", "typecheck"], ts));
        checks.push(await shGate("biome check", "pnpm", ["exec", "biome", "check", "pkg-baml-tooling"], ts));
        return { ok: true as const, checks, notes: "mechanical repo gates green before review" };
      }}
    </Task>
  );
}
