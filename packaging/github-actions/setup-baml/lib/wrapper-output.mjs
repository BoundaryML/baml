import process from "node:process";
import { pathToFileURL } from "node:url";

const exactVersionPattern = /^[0-9][0-9A-Za-z.+_-]{0,127}$/;
const selectorPattern = /^(?:canary|nightly|[0-9][0-9A-Za-z.+_-]{0,127})$/;

function validateExactVersion(value) {
  const version = value.trim();
  if (!exactVersionPattern.test(version)) {
    throw new Error("the BAML wrapper did not report a resolved concrete toolchain version");
  }
  return version;
}

export function validateSelector(value) {
  const selector = value.trim();
  if (!selectorPattern.test(selector)) {
    throw new Error("toolchain must be canary, nightly, or an exact version containing only letters, digits, '.', '+', '_', and '-'");
  }
  return selector;
}

export function resolveSelector(output) {
  const unresolved = output.match(/^active selector: \(unresolved\)$/m);
  if (unresolved) {
    throw new Error("the BAML wrapper could not resolve the project toolchain selector");
  }
  const active = output.match(/^active selector: ([^\s(]+)(?:\s|$)/m);
  const configuredDefault = output.match(/^default selector: ([^\s]+)$/m);
  const selector = active?.[1] ?? configuredDefault?.[1];
  if (!selector) {
    throw new Error("the BAML wrapper did not report an active or default selector");
  }
  return validateSelector(selector);
}

export function resolveVersion(output) {
  const match = output.match(/^baml toolchain ([^\s(]+)(?:\s|$)/m);
  if (!match || match[1] === "not") {
    throw new Error("the BAML wrapper did not report a resolved concrete toolchain version");
  }
  return validateExactVersion(match[1]);
}

async function main() {
  const mode = process.argv[2];
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  const input = Buffer.concat(chunks).toString("utf8");
  switch (mode) {
    case "validate-selector":
      process.stdout.write(`${validateSelector(input)}\n`);
      break;
    case "selector":
      process.stdout.write(`${resolveSelector(input)}\n`);
      break;
    case "version":
      process.stdout.write(`${resolveVersion(input)}\n`);
      break;
    default:
      throw new Error("usage: wrapper-output.mjs <validate-selector|selector|version>");
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    process.stderr.write(`setup-baml: ${error.message}\n`);
    process.exitCode = 1;
  });
}
