import assert from "node:assert/strict";
import test from "node:test";

import { resolveSelector, resolveVersion, validateSelector } from "../lib/wrapper-output.mjs";

test("accepts supported channels and exact versions", () => {
  for (const selector of ["canary", "nightly", "0.16.0", "0.15.1-nightly.20260810.b", "1.0.0+build_7"]) {
    assert.equal(validateSelector(selector), selector);
  }
});

test("rejects paths, shell syntax, whitespace, and empty selectors", () => {
  for (const selector of ["", "../baml-cli", "1.2.3; echo bad", "1.2.3\nother", "v1.2.3"]) {
    assert.throws(() => validateSelector(selector));
  }
});

test("uses the wrapper default when there is no project override", () => {
  assert.equal(resolveSelector("default selector: canary\ninstalled toolchains: (none)\n"), "canary");
  assert.equal(resolveSelector("default selector: nightly\ninstalled toolchains: (none)\n"), "nightly");
});

test("reads the project selector selected by the wrapper", () => {
  const output = "default selector: canary\nactive selector: nightly (from /work/baml.toml)\ninstalled toolchains: (none)\n";
  assert.equal(resolveSelector(output), "nightly");
});

test("rejects unresolved and unsafe wrapper selectors", () => {
  assert.throws(() => resolveSelector("active selector: (unresolved)\nfailed to parse baml.toml\n"));
  assert.throws(() => resolveSelector("active selector: ../../baml-cli (from baml.toml)\n"));
  assert.throws(() => resolveSelector("installed toolchains: (none)\n"));
});

test("reads concrete versions from wrapper version output", () => {
  assert.equal(resolveVersion("baml wrapper 0.2.4\nbaml toolchain 0.16.0 (nightly)\n"), "0.16.0");
  assert.equal(resolveVersion("baml wrapper 0.2.4\nbaml toolchain 0.15.1-nightly.20260810.b (nightly, from /work/baml.toml)\n"), "0.15.1-nightly.20260810.b");
});

test("rejects unresolved version output", () => {
  assert.throws(() => resolveVersion("baml wrapper 0.2.4\nbaml toolchain not installed\n"));
  assert.throws(() => resolveVersion("baml wrapper 0.2.4\nbaml toolchain canary\n"));
  assert.throws(() => resolveVersion("baml wrapper 0.2.4\nbaml toolchain nightly\n"));
});
