import { spawn } from "node:child_process";
import { createReadStream, createWriteStream } from "node:fs";

// The archive root is passed via the child's working directory (spawn's `cwd`
// option), NOT tar's `-C` flag. Node sets the cwd through the OS natively, so a
// Windows path like `C:\Users\runner\.cargo` is handled correctly; passing the
// same path to Git-for-Windows' MSYS `tar -C` mangles the backslashes/`:` and
// fails ("Cannot open"). Archives are also fed/emitted on stdio (`-f -`) rather
// than a `-f <path>` arg, dodging GNU tar's `host:file` parsing of a `C:` path.

/**
 * Create an (uncompressed) tar of `relPath` rooted at `cwd`, streamed to
 * `destPath` on disk. Used for git db dirs that can be multi-GB — we never hold
 * the archive in memory; the caller multipart-uploads the file from disk.
 * Returns the archive size in bytes.
 */
export function tarCreateToFile(cwd: string, relPath: string, destPath: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const child = spawn("tar", ["-cf", "-", relPath], {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const out = createWriteStream(destPath);
    const errChunks: Buffer[] = [];
    let failed = false;
    let exited = false;
    let finished = false;
    const fail = (e: Error): void => {
      if (failed) return;
      failed = true;
      child.kill();
      out.destroy();
      reject(e);
    };
    // Resolve only once BOTH the child has exited 0 and the file stream has
    // flushed — these two events race, so we can't key off either one alone.
    const maybeDone = (): void => {
      if (!failed && exited && finished) resolve(out.bytesWritten);
    };
    child.stderr.on("data", (c: Buffer) => errChunks.push(c));
    child.on("error", fail);
    out.on("error", fail);
    out.on("finish", () => {
      finished = true;
      maybeDone();
    });
    child.stdout.pipe(out);
    child.on("close", (code) => {
      if (code !== 0) {
        fail(new Error(`tar create failed (${code}): ${Buffer.concat(errChunks)}`));
        return;
      }
      exited = true;
      maybeDone();
    });
  });
}

/**
 * Create an (uncompressed) tar of `relPath` rooted at `cwd`, returned as a Buffer.
 * Git object stores are already zlib-compressed, so we skip a second compression
 * pass — it would burn CPU for almost no size win. The archive preserves the
 * relative path so it restores into exactly the right place.
 */
export function tarCreate(cwd: string, relPath: string): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const child = spawn("tar", ["-cf", "-", relPath], {
      cwd,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const chunks: Buffer[] = [];
    const errChunks: Buffer[] = [];
    child.stdout.on("data", (c: Buffer) => chunks.push(c));
    child.stderr.on("data", (c: Buffer) => errChunks.push(c));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) resolve(Buffer.concat(chunks));
      else reject(new Error(`tar create failed (${code}): ${Buffer.concat(errChunks)}`));
    });
  });
}

/**
 * Extract the tar archive at `filePath` into `cwd`, streaming the file in via
 * tar's stdin (`-xf -`). Streaming keeps memory flat for multi-GB git db tars.
 */
export function tarExtractFromFile(cwd: string, filePath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn("tar", ["-xf", "-"], {
      cwd,
      stdio: ["pipe", "ignore", "pipe"],
    });
    const errChunks: Buffer[] = [];
    let failed = false;
    const fail = (e: Error): void => {
      if (failed) return;
      failed = true;
      child.kill();
      reject(e);
    };
    child.stderr.on("data", (c: Buffer) => errChunks.push(c));
    child.on("error", fail);
    child.on("close", (code) => {
      if (failed) return;
      if (code === 0) resolve();
      else fail(new Error(`tar extract failed (${code}): ${Buffer.concat(errChunks)}`));
    });
    const rs = createReadStream(filePath);
    rs.on("error", fail);
    // tar may exit (and close stdin) before we finish writing; swallow EPIPE.
    child.stdin.on("error", () => {});
    rs.pipe(child.stdin);
  });
}

/** Extract a tar buffer into `cwd`. */
export function tarExtract(cwd: string, data: Buffer): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn("tar", ["-xf", "-"], {
      cwd,
      stdio: ["pipe", "ignore", "pipe"],
    });
    const errChunks: Buffer[] = [];
    child.stderr.on("data", (c: Buffer) => errChunks.push(c));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) resolve();
      else reject(new Error(`tar extract failed (${code}): ${Buffer.concat(errChunks)}`));
    });
    child.stdin.write(data);
    child.stdin.end();
  });
}
