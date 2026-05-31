import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash, randomBytes } from "node:crypto";
import * as os from "node:os";
import * as fsp from "node:fs/promises";
import * as path from "node:path";
import { resolveR2Config, R2Store } from "../src/r2.ts";
import { tarCreateToFile, tarExtractFromFile } from "../src/tar.ts";

/**
 * Integration tests for the large-object upload path, against the *real* R2
 * bucket. Writes under a unique throwaway prefix and cleans up after itself.
 * Skips when credentials aren't present (fork PR / no direnv export).
 */
const cfg = resolveR2Config(`__selftest__/${process.pid}-${randomBytes(6).toString("hex")}`);
const skip = cfg ? false : "no R2 credentials in env";
const FIVE_MIB = 5 * 1024 * 1024;
const sha = (b: Buffer): string => createHash("sha256").update(b).digest("hex");

test("putLargeFile: multipart upload round-trips a >part-size file", { skip }, async () => {
  const store = new R2Store(cfg!);
  // 13 MiB with a 5 MiB part size => 3 parts (5 + 5 + 3), exercising the
  // multipart init / presigned part PUT / complete flow.
  const size = 13 * 1024 * 1024;
  const payload = randomBytes(size);
  const tmp = path.join(os.tmpdir(), `mp-${randomBytes(8).toString("hex")}.bin`);
  await fsp.writeFile(tmp, payload);
  const key = `multipart/${randomBytes(8).toString("hex")}.bin`;

  try {
    await store.putLargeFile(key, tmp, "application/octet-stream", {
      partSize: FIVE_MIB,
      threshold: FIVE_MIB,
    });
    const got = await store.get(key);
    assert.ok(got, "object should exist after multipart upload");
    assert.equal(got!.length, size, "length must match");
    assert.equal(sha(got!), sha(payload), "bytes must round-trip exactly");
  } finally {
    await store.del(key).catch(() => {});
    await fsp.rm(tmp, { force: true });
  }
});

test("putLargeFile: a file at/under the threshold uses a single PUT", { skip }, async () => {
  const store = new R2Store(cfg!);
  const payload = randomBytes(1024);
  const tmp = path.join(os.tmpdir(), `mp-small-${randomBytes(8).toString("hex")}.bin`);
  await fsp.writeFile(tmp, payload);
  const key = `multipart/small-${randomBytes(8).toString("hex")}.bin`;

  try {
    await store.putLargeFile(key, tmp, "application/octet-stream", {
      partSize: FIVE_MIB,
      threshold: FIVE_MIB,
    });
    const got = await store.get(key);
    assert.ok(got && sha(got) === sha(payload), "small file round-trips via single PUT");
  } finally {
    await store.del(key).catch(() => {});
    await fsp.rm(tmp, { force: true });
  }
});

test("getToFile: streams an object back to disk byte-identical", { skip }, async () => {
  const store = new R2Store(cfg!);
  const size = 11 * 1024 * 1024; // > one 5 MiB part, exercises a real stream
  const payload = randomBytes(size);
  const up = path.join(os.tmpdir(), `gtf-up-${randomBytes(8).toString("hex")}.bin`);
  const down = path.join(os.tmpdir(), `gtf-down-${randomBytes(8).toString("hex")}.bin`);
  await fsp.writeFile(up, payload);
  const key = `getfile/${randomBytes(8).toString("hex")}.bin`;

  try {
    await store.putLargeFile(key, up, "application/octet-stream", {
      partSize: FIVE_MIB,
      threshold: FIVE_MIB,
    });
    const present = await store.getToFile(key, down);
    assert.equal(present, true, "getToFile reports the object as present");
    assert.equal(sha(await fsp.readFile(down)), sha(payload), "streamed bytes match");

    // A missing key streams to false (and writes no usable file).
    const missing = await store.getToFile(`getfile/missing-${randomBytes(6).toString("hex")}`, down + ".x");
    assert.equal(missing, false, "getToFile returns false for a missing object");
  } finally {
    await store.del(key).catch(() => {});
    await fsp.rm(up, { force: true });
    await fsp.rm(down, { force: true });
    await fsp.rm(down + ".x", { force: true });
  }
});

test("tarCreateToFile + tarExtractFromFile round-trip a directory tree", async () => {
  const base = await fsp.mkdtemp(path.join(os.tmpdir(), "tarstream-"));
  const srcRoot = path.join(base, "src");
  const dstRoot = path.join(base, "dst");
  await fsp.mkdir(path.join(srcRoot, "git", "db", "repo-xyz", "objects"), { recursive: true });
  await fsp.writeFile(path.join(srcRoot, "git", "db", "repo-xyz", "HEAD"), "ref: refs/heads/main\n");
  await fsp.writeFile(path.join(srcRoot, "git", "db", "repo-xyz", "objects", "p.idx"), Buffer.from([0, 255, 0, 66]));
  await fsp.mkdir(dstRoot, { recursive: true });

  const tar = path.join(base, "repo.tar");
  const bytes = await tarCreateToFile(srcRoot, ["git", "db", "repo-xyz"].join("/"), tar);
  assert.ok(bytes > 0, "archive has content");
  await tarExtractFromFile(dstRoot, tar);

  const head = await fsp.readFile(path.join(dstRoot, "git", "db", "repo-xyz", "HEAD"), "utf8");
  assert.equal(head, "ref: refs/heads/main\n");
  const idx = await fsp.readFile(path.join(dstRoot, "git", "db", "repo-xyz", "objects", "p.idx"));
  assert.deepEqual([...idx], [0, 255, 0, 66]);

  await fsp.rm(base, { recursive: true, force: true });
});

test("R2Store: a key containing '+' signs correctly (encoding regression)", { skip }, async () => {
  // Crate files like `foo-1.0.0+build.crate` carry a `+`; if the wire path isn't
  // encoded the same way the signature is, R2 returns 403 SignatureDoesNotMatch.
  const store = new R2Store(cfg!);
  const key = `plus-test/ash-0.38.0+1.3.281-${randomBytes(6).toString("hex")}.crate`;
  const payload = randomBytes(256);
  try {
    assert.equal(await store.has(key), false, "missing '+' key HEADs as absent (404, not 403)");
    await store.put(key, payload, "application/x-tar");
    assert.equal(await store.has(key), true);
    const got = await store.get(key);
    assert.ok(got && got.equals(payload), "'+' key round-trips");
  } finally {
    await store.del(key).catch(() => {});
  }
});
