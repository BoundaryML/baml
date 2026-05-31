import { test } from "node:test";
import assert from "node:assert/strict";
import * as os from "node:os";
import * as fsp from "node:fs/promises";
import * as path from "node:path";
import { resolveR2Config, joinKey } from "../src/r2.ts";
import { tarCreate, tarExtract } from "../src/tar.ts";

function withEnv(vars: Record<string, string>, fn: () => void): void {
  const saved = { ...process.env };
  for (const k of Object.keys(process.env)) {
    if (/SCCACHE|R2|AWS/i.test(k)) delete process.env[k];
  }
  Object.assign(process.env, vars);
  try {
    fn();
  } finally {
    for (const k of Object.keys(process.env)) delete process.env[k];
    Object.assign(process.env, saved);
  }
}

test("resolveR2Config: picks up SCCACHE_*R2_* variables", () => {
  withEnv(
    {
      SCCACHE_R2_ENDPOINT: "https://acct.r2.cloudflarestorage.com",
      SCCACHE_R2_BUCKET: "ci-cache",
      SCCACHE_R2_ACCESS_KEY_ID: "AKID",
      SCCACHE_R2_SECRET_ACCESS_KEY: "SECRET",
    },
    () => {
      const cfg = resolveR2Config("cargo-home");
      assert.ok(cfg);
      assert.equal(cfg!.endpoint, "https://acct.r2.cloudflarestorage.com");
      assert.equal(cfg!.bucket, "ci-cache");
      assert.equal(cfg!.accessKeyId, "AKID");
      assert.equal(cfg!.secretAccessKey, "SECRET");
      assert.equal(cfg!.region, "auto");
      assert.equal(cfg!.keyPrefix, "cargo-home");
    },
  );
});

test("resolveR2Config: BAML_SCCACHE_R2_* creds + standard sccache endpoint/bucket", () => {
  withEnv(
    {
      SCCACHE_ENDPOINT: "https://acct.r2.cloudflarestorage.com",
      SCCACHE_BUCKET: "baml-build1",
      SCCACHE_S3_KEY_PREFIX: "baml/local/",
      BAML_SCCACHE_R2_ACCESS_KEY_ID: "k",
      BAML_SCCACHE_R2_SECRET_ACCESS_KEY: "s",
    },
    () => {
      const cfg = resolveR2Config("cargo-home");
      assert.ok(cfg);
      assert.equal(cfg!.bucket, "baml-build1");
      assert.equal(cfg!.accessKeyId, "k");
      assert.equal(cfg!.secretAccessKey, "s");
      assert.equal(cfg!.keyPrefix, "baml/local/cargo-home");
    },
  );
});

test("resolveR2Config: tolerates alternate spellings + adds https", () => {
  withEnv(
    {
      SCCACHE_S3_USE_R2_ENDPOINT: "acct.r2.cloudflarestorage.com",
      SCCACHE_R2_BUCKET_NAME: "b",
      MY_SCCACHE_R2_ACCESS_KEY_ID: "k",
      MY_SCCACHE_R2_SECRET_ACCESS_KEY: "s",
    },
    () => {
      const cfg = resolveR2Config("cargo-home");
      assert.ok(cfg);
      assert.equal(cfg!.endpoint, "https://acct.r2.cloudflarestorage.com");
      assert.equal(cfg!.bucket, "b");
    },
  );
});

test("resolveR2Config: falls back to AWS_* creds", () => {
  withEnv(
    {
      SCCACHE_ENDPOINT: "https://acct.r2.cloudflarestorage.com",
      SCCACHE_BUCKET: "bkt",
      AWS_ACCESS_KEY_ID: "aws-key",
      AWS_SECRET_ACCESS_KEY: "aws-secret",
    },
    () => {
      const cfg = resolveR2Config("p");
      assert.ok(cfg);
      assert.equal(cfg!.accessKeyId, "aws-key");
      assert.equal(cfg!.secretAccessKey, "aws-secret");
    },
  );
});

test("resolveR2Config: returns null when creds are incomplete", () => {
  withEnv({ SCCACHE_R2_BUCKET: "b" }, () => {
    assert.equal(resolveR2Config("p"), null);
  });
});

test("joinKey trims and joins", () => {
  assert.equal(joinKey("a/", "/b/", "c"), "a/b/c");
  assert.equal(joinKey("", "x"), "x");
});

test("tar: create then extract round-trips a directory tree", async () => {
  const base = await fsp.mkdtemp(path.join(os.tmpdir(), "tartest-"));
  const srcRoot = path.join(base, "src");
  const dstRoot = path.join(base, "dst");
  await fsp.mkdir(path.join(srcRoot, "git", "db", "repo-abc", "objects"), { recursive: true });
  await fsp.writeFile(path.join(srcRoot, "git", "db", "repo-abc", "HEAD"), "ref: refs/heads/main\n");
  await fsp.writeFile(path.join(srcRoot, "git", "db", "repo-abc", "objects", "pack.idx"), "binary\0data");
  await fsp.mkdir(dstRoot, { recursive: true });

  const buf = await tarCreate(srcRoot, path.join("git", "db", "repo-abc"));
  assert.ok(buf.length > 0);
  await tarExtract(dstRoot, buf);

  const head = await fsp.readFile(path.join(dstRoot, "git", "db", "repo-abc", "HEAD"), "utf8");
  assert.equal(head, "ref: refs/heads/main\n");
  const pack = await fsp.readFile(path.join(dstRoot, "git", "db", "repo-abc", "objects", "pack.idx"));
  assert.equal(pack.toString("binary"), "binary\0data");

  await fsp.rm(base, { recursive: true, force: true });
});
