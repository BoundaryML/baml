// Deterministic helpers shared by the task modules (compute tasks and prompt
// builders). Workflow scaffolding; removed by the final cleanup task.
import { execFile, execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { promisify } from "node:util";
import { REPO_ROOT } from "./constants.ts";

export function sh(cmd: string, args: string[], opts: { allowFail?: boolean } = {}): string {
  try {
    return execFileSync(cmd, args, {
      cwd: REPO_ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 64 * 1024 * 1024,
    });
  } catch (error) {
    if (opts.allowFail) return "";
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`command failed: ${cmd} ${args.join(" ")}\n${detail}`);
  }
}
export function trySh(cmd: string, args: string[]): string | null {
  try {
    return execFileSync(cmd, args, {
      cwd: REPO_ROOT,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      maxBuffer: 64 * 1024 * 1024,
    });
  } catch {
    return null;
  }
}
export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

// Async runner for the ci-gate: long checks (clippy) must not block the engine's
// event loop the way execFileSync would (retro change 5).
const execFileAsync = promisify(execFile);
export async function shGate(label: string, cmd: string, args: string[], cwd: string): Promise<string> {
  try {
    await execFileAsync(cmd, args, { cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
    return `${label}: PASS`;
  } catch (error) {
    const e = error as { stdout?: string; stderr?: string; message?: string };
    const detail = [e.stderr, e.stdout, e.message].filter(Boolean).join("\n").slice(0, 4000);
    throw new Error(`ci-gate check failed — ${label}: ${cmd} ${args.join(" ")}\n${detail}`);
  }
}

// Output rows come back DB-shaped: possibly `.row`-wrapped, snake_case, booleans as
// 0/1, arrays as JSON strings. Read everything defensively.
export function unwrap(row: unknown): Record<string, unknown> {
  const r = row as Record<string, unknown> | null | undefined;
  if (r && typeof r === "object" && "row" in r && r.row && typeof r.row === "object") {
    return r.row as Record<string, unknown>;
  }
  return (r ?? {}) as Record<string, unknown>;
}
export function pick(row: unknown, camel: string): unknown {
  const r = unwrap(row);
  if (camel in r) return r[camel];
  const snake = camel.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
  return r[snake];
}
export function asStr(row: unknown, key: string, fallback = ""): string {
  const v = pick(row, key);
  return typeof v === "string" ? v : fallback;
}
export function asArr(row: unknown, key: string): unknown[] {
  const v = pick(row, key);
  if (Array.isArray(v)) return v;
  if (typeof v === "string") {
    try {
      const parsed = JSON.parse(v);
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }
  return [];
}
export function asBool(row: unknown, key: string): boolean {
  const v = pick(row, key);
  return v === true || v === 1 || v === "1" || v === "true";
}

export function readArtifact(repoRelPath: string, cap = 180_000): string {
  const abs = resolve(REPO_ROOT, repoRelPath);
  try {
    const text = readFileSync(abs, "utf8");
    if (text.length <= cap) return text;
    return `${text.slice(0, cap)}\n\n[TRUNCATED at ${cap} of ${text.length} characters]`;
  } catch {
    return `[artifact ${repoRelPath} could not be read]`;
  }
}
