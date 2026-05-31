import { appendFileSync } from "node:fs";
import { randomUUID } from "node:crypto";

/**
 * The handful of GitHub Actions toolkit helpers this action uses, reimplemented
 * directly against the documented runner protocol (workflow commands + the
 * GITHUB_OUTPUT / GITHUB_STATE files). This avoids bundling @actions/core, whose
 * OIDC/platform helpers drag in @actions/http-client/exec/io and balloon the
 * committed bundle by ~450KB for five trivial functions.
 *
 * Protocol reference:
 *   https://docs.github.com/actions/using-workflows/workflow-commands-for-github-actions
 */

function escapeData(s: string): string {
  return s.replace(/%/g, "%25").replace(/\r/g, "%0D").replace(/\n/g, "%0A");
}

/** Append a `key<<delimiter ... delimiter` block to a runner env file. */
function appendKeyValue(file: string, key: string, value: string): void {
  const delimiter = `ghadelimiter_${randomUUID()}`;
  appendFileSync(file, `${key}<<${delimiter}\n${value}\n${delimiter}\n`);
}

export function info(message: string): void {
  process.stdout.write(`${message}\n`);
}

export function warning(message: string): void {
  process.stdout.write(`::warning::${escapeData(message)}\n`);
}

/** Set a step output (`outputs.<name>`). */
export function setOutput(name: string, value: string): void {
  const file = process.env.GITHUB_OUTPUT;
  if (file) {
    appendKeyValue(file, name, value);
  } else {
    // Fallback for non-runner contexts (local debugging).
    process.stdout.write(`::set-output name=${name}::${escapeData(value)}\n`);
  }
}

/** Persist state for the action's post step to read via getState. */
export function saveState(name: string, value: string): void {
  const file = process.env.GITHUB_STATE;
  if (file) {
    appendKeyValue(file, name, value);
  } else {
    process.stdout.write(`::save-state name=${name}::${escapeData(value)}\n`);
  }
}

/** Read state saved by the main step (the runner exposes it as STATE_<name>). */
export function getState(name: string): string {
  return process.env[`STATE_${name}`] ?? "";
}
