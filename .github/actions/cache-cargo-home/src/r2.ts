import { Agent as HttpsAgent, request as httpsRequest } from "node:https";
import { Agent as HttpAgent, request as httpRequest } from "node:http";
import * as fsp from "node:fs/promises";
import { createWriteStream } from "node:fs";
import { createHash } from "node:crypto";
import { signRequest, presign, encodePath, canonicalQuery } from "./sigv4.ts";
import { mapPool } from "./pool.ts";

/** S3 multipart part size. 64 MiB keeps each in-flight part well under the ~2GB
 *  hash limit and stays above S3's 5 MiB part minimum. */
export const MULTIPART_PART_SIZE = 64 * 1024 * 1024;
/** Objects at/under this size go up in a single PUT; larger ones use multipart. */
export const MULTIPART_THRESHOLD = MULTIPART_PART_SIZE;
/** How many parts to upload concurrently. */
const MULTIPART_CONCURRENCY = 4;

export interface R2Config {
  endpoint: string;
  bucket: string;
  accessKeyId: string;
  secretAccessKey: string;
  region: string;
  /** Optional prefix prepended to every object key. */
  keyPrefix: string;
}

/**
 * Resolve R2 / S3 connection settings from the environment.
 *
 * The user's CI exports a set of `SCCACHE_*R2_*` variables (their exact spelling
 * varies), so rather than hard-coding names we scan for anything matching that
 * pattern and classify it by suffix, then fall back to the standard sccache S3
 * names and finally to the conventional `AWS_*` / `R2_*` names. This keeps the
 * action working regardless of the precise variable spellings in use.
 */
export function resolveR2Config(extraPrefix: string): R2Config | null {
  const env = process.env;

  // 1. Collect anything that looks like an sccache R2 variable.
  const scoped: Record<string, string> = {};
  for (const [k, v] of Object.entries(env)) {
    if (!v) continue;
    if (/sccache/i.test(k) && /r2/i.test(k)) {
      scoped[k.toUpperCase()] = v;
    }
  }

  const findScoped = (...needles: string[]): string | undefined => {
    for (const [k, v] of Object.entries(scoped)) {
      if (needles.every((n) => k.includes(n))) return v;
    }
    return undefined;
  };

  const first = (...vals: Array<string | undefined>): string | undefined =>
    vals.find((v) => v !== undefined && v !== "");

  const endpoint = first(
    findScoped("ENDPOINT"),
    env.SCCACHE_ENDPOINT,
    env.R2_ENDPOINT,
    env.AWS_ENDPOINT_URL_S3,
    env.AWS_ENDPOINT_URL,
  );

  const bucket = first(
    findScoped("BUCKET"),
    env.SCCACHE_BUCKET,
    env.R2_BUCKET,
    env.AWS_S3_BUCKET,
  );

  const accessKeyId = first(
    findScoped("ACCESS_KEY_ID"),
    // Note: ACCESS_KEY_ID check must precede SECRET below since both contain
    // "ACCESS_KEY"; we match on the distinguishing "_ID".
    env.R2_ACCESS_KEY_ID,
    env.AWS_ACCESS_KEY_ID,
  );

  const secretAccessKey = first(
    findScoped("SECRET_ACCESS_KEY"),
    findScoped("SECRET"),
    env.R2_SECRET_ACCESS_KEY,
    env.AWS_SECRET_ACCESS_KEY,
  );

  const region = first(
    findScoped("REGION"),
    env.SCCACHE_REGION,
    env.AWS_REGION,
    env.AWS_DEFAULT_REGION,
  ) ?? "auto"; // R2 ignores region but SigV4 still requires one in the scope.

  // An optional shared prefix (matches sccache's own key namespacing).
  const basePrefix = first(findScoped("KEY_PREFIX"), env.SCCACHE_S3_KEY_PREFIX) ?? "";
  const keyPrefix = joinKey(basePrefix, extraPrefix);

  if (!endpoint || !bucket || !accessKeyId || !secretAccessKey) {
    return null;
  }

  let normalizedEndpoint = endpoint;
  if (!/^https?:\/\//.test(normalizedEndpoint)) {
    normalizedEndpoint = `https://${normalizedEndpoint}`;
  }

  return {
    endpoint: normalizedEndpoint,
    bucket,
    accessKeyId,
    secretAccessKey,
    region,
    keyPrefix,
  };
}

export function joinKey(...parts: string[]): string {
  return parts
    .map((p) => p.replace(/^\/+|\/+$/g, ""))
    .filter((p) => p.length > 0)
    .join("/");
}

/** Small backoff between retry attempts (0-indexed): 100ms, 200ms, 300ms. */
function delay(attempt: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 100 * (attempt + 1)));
}

interface RawResponse {
  status: number;
  body: Buffer;
  headers: Record<string, string | string[] | undefined>;
}

export class R2Store {
  readonly bucket: string;
  readonly keyPrefix: string;
  private readonly endpoint: URL;
  private readonly region: string;
  private readonly accessKeyId: string;
  private readonly secretAccessKey: string;
  private readonly agent: HttpsAgent | HttpAgent;
  private readonly request: typeof httpsRequest;

  constructor(cfg: R2Config) {
    this.bucket = cfg.bucket;
    this.keyPrefix = cfg.keyPrefix;
    this.endpoint = new URL(cfg.endpoint);
    this.region = cfg.region;
    this.accessKeyId = cfg.accessKeyId;
    this.secretAccessKey = cfg.secretAccessKey;
    // Honour the endpoint scheme: https in production R2, http for a local mock.
    const isHttps = this.endpoint.protocol === "https:";
    // keepAlive is intentionally OFF: a pooled idle socket that R2 resets emits
    // an 'error' with no request bound to it, which Node turns into an uncaught
    // exception ("write EOF") that killed the step in CI. A fresh connection per
    // request sidesteps that entirely; with high maxSockets the parallel TLS
    // handshakes cost ~1-2s total, negligible next to a cold `cargo fetch`.
    const opts = { keepAlive: false, maxSockets: 64 };
    this.agent = isHttps ? new HttpsAgent(opts) : new HttpAgent(opts);
    this.request = isHttps ? httpsRequest : httpRequest;
  }

  private fullKey(key: string): string {
    return joinKey(this.keyPrefix, key);
  }

  /** Build the path-style object URL (R2 prefers path-style addressing). */
  private objectUrl(key: string): URL {
    const url = new URL(this.endpoint.toString());
    url.pathname = "/" + joinKey(this.bucket, this.fullKey(key));
    return url;
  }

  /** Sign + send one request to a key, retrying transient failures. */
  private send(
    method: "GET" | "PUT" | "HEAD" | "DELETE" | "POST",
    key: string,
    body?: Buffer,
    contentType?: string,
  ): Promise<RawResponse> {
    return this.sendUrl(method, this.objectUrl(key), body, contentType);
  }

  /** Sign + send one request to a fully-formed URL, retrying transient failures. */
  private async sendUrl(
    method: "GET" | "PUT" | "HEAD" | "DELETE" | "POST",
    url: URL,
    body?: Buffer,
    contentType?: string,
  ): Promise<RawResponse> {
    const extraHeaders: Record<string, string> = {};
    if (contentType) extraHeaders["content-type"] = contentType;

    // Signed once and reused across retries — well within SigV4's clock-skew
    // window given our 120s per-attempt timeout.
    const headers = signRequest({
      method,
      url,
      region: this.region,
      accessKeyId: this.accessKeyId,
      secretAccessKey: this.secretAccessKey,
      body,
      headers: extraHeaders,
    });

    let lastErr: unknown;
    for (let attempt = 0; attempt < 4; attempt++) {
      try {
        const res = await this.once(method, url, headers, body);
        // Retry 5xx (transient); return everything else to the caller.
        if (res.status >= 500 && attempt < 3) {
          lastErr = new Error(`HTTP ${res.status}`);
          await delay(attempt);
          continue;
        }
        return res;
      } catch (e) {
        // Connection-level errors (EOF / ECONNRESET from a recycled keep-alive
        // socket) land here; retry on a fresh connection.
        lastErr = e;
        if (attempt < 3) await delay(attempt);
      }
    }
    throw lastErr instanceof Error ? lastErr : new Error(String(lastErr));
  }

  private once(
    method: string,
    url: URL,
    headers: Record<string, string>,
    body?: Buffer,
  ): Promise<RawResponse> {
    // The wire path must use the SAME encoding the signature used (AWS path +
    // canonical query), or reserved chars like `+` in a key, or `/` in an
    // uploadId, yield a 403 SignatureDoesNotMatch.
    const q = canonicalQuery(url.searchParams);
    const wirePath = encodePath(url.pathname) + (q ? `?${q}` : "");
    return this.exec(method, url.hostname, url.port, wirePath, headers, body);
  }

  /** Low-level single HTTP request to a fully-formed wire path. */
  private exec(
    method: string,
    hostname: string,
    port: string,
    wirePath: string,
    headers: Record<string, string>,
    body?: Buffer,
  ): Promise<RawResponse> {
    const defaultPort = this.endpoint.protocol === "https:" ? 443 : 80;
    return new Promise((resolve, reject) => {
      let settled = false;
      const ok = (r: RawResponse): void => {
        if (!settled) {
          settled = true;
          resolve(r);
        }
      };
      const fail = (e: Error): void => {
        if (!settled) {
          settled = true;
          reject(e);
        }
      };

      const req = this.request(
        {
          method,
          hostname,
          port: port || defaultPort,
          path: wirePath,
          headers: { ...headers, "content-length": String(body?.length ?? 0) },
          agent: this.agent,
          timeout: 120_000,
        },
        (res) => {
          const chunks: Buffer[] = [];
          res.on("data", (c: Buffer) => chunks.push(c));
          res.on("end", () =>
            ok({ status: res.statusCode ?? 0, body: Buffer.concat(chunks), headers: res.headers }),
          );
          res.on("error", fail);
        },
      );
      req.on("error", fail);
      req.on("timeout", () => req.destroy(new Error("request timed out")));
      // A socket can emit 'error' (EOF / ECONNRESET) at a point where it isn't
      // routed to the request — which Node turns into an uncaughtException that
      // killed the whole restore on Windows ("write EOF"). Keep an error
      // listener on the socket for its entire life (sockets aren't pooled —
      // keepAlive is off) so such an error is always handled; once we've settled
      // the `settled` guard makes it a no-op instead of a retry.
      req.on("socket", (s) => {
        s.on("error", (e: Error) => fail(e));
      });
      if (body && body.length > 0) req.write(body);
      req.end();
    });
  }

  /**
   * List object keys (relative to keyPrefix) under a prefix via ListObjectsV2.
   * Used to discover a git db tarball whose cargo-hash suffix we can't predict.
   */
  async list(prefix: string): Promise<string[]> {
    const url = new URL(this.endpoint.toString());
    url.pathname = "/" + joinKey(this.bucket);
    const fullPrefix = joinKey(this.keyPrefix, prefix);
    url.searchParams.set("list-type", "2");
    url.searchParams.set("prefix", fullPrefix);

    const res = await this.sendUrl("GET", url);
    if (res.status < 200 || res.status >= 300) {
      throw new Error(`R2 LIST ${prefix} failed: HTTP ${res.status} ${res.body.subarray(0, 300)}`);
    }
    const xml = res.body.toString("utf8");
    const strip = this.keyPrefix ? `${this.keyPrefix}/` : "";
    const keys: string[] = [];
    for (const m of xml.matchAll(/<Key>([^<]+)<\/Key>/g)) {
      let k = m[1]!;
      if (strip && k.startsWith(strip)) k = k.slice(strip.length);
      keys.push(k);
    }
    return keys;
  }

  /** Download an object's bytes, or null if it does not exist. */
  async get(key: string): Promise<Buffer | null> {
    const res = await this.send("GET", key);
    if (res.status === 404) return null;
    if (res.status >= 200 && res.status < 300) return res.body;
    throw httpError("GET", key, res);
  }

  /**
   * Stream an object to `destPath` on disk, returning false if it doesn't exist.
   * Used for the multi-GB git db tars: a plain `get()` would buffer the whole
   * object in memory (and `Buffer.concat` caps out near 4 GB). Each retry
   * re-opens (truncates) the destination, so a reset mid-download restarts clean.
   */
  async getToFile(key: string, destPath: string): Promise<boolean> {
    const url = this.objectUrl(key);
    // Signed once and reused across retries (GET body is empty; well within the
    // SigV4 clock-skew window given the per-attempt timeout).
    const headers = signRequest({
      method: "GET",
      url,
      region: this.region,
      accessKeyId: this.accessKeyId,
      secretAccessKey: this.secretAccessKey,
    });
    const q = canonicalQuery(url.searchParams);
    const wirePath = encodePath(url.pathname) + (q ? `?${q}` : "");

    let lastErr: unknown;
    for (let attempt = 0; attempt < 4; attempt++) {
      try {
        const status = await this.streamToFile(url.hostname, url.port, wirePath, headers, destPath);
        if (status === 404) return false;
        if (status >= 200 && status < 300) return true;
        if (status >= 500 && attempt < 3) {
          lastErr = new Error(`HTTP ${status}`);
          await delay(attempt);
          continue;
        }
        throw new Error(`R2 GET ${key} failed: HTTP ${status}`);
      } catch (e) {
        lastErr = e;
        if (attempt < 3) await delay(attempt);
      }
    }
    throw lastErr instanceof Error ? lastErr : new Error(String(lastErr));
  }

  /** One streaming GET: pipe a 2xx body to `destPath`; resolve the status code. */
  private streamToFile(
    hostname: string,
    port: string,
    wirePath: string,
    headers: Record<string, string>,
    destPath: string,
  ): Promise<number> {
    const defaultPort = this.endpoint.protocol === "https:" ? 443 : 80;
    return new Promise((resolve, reject) => {
      let settled = false;
      const fail = (e: Error): void => {
        if (!settled) {
          settled = true;
          reject(e);
        }
      };
      const req = this.request(
        {
          method: "GET",
          hostname,
          port: port || defaultPort,
          path: wirePath,
          headers: { ...headers, "content-length": "0" },
          agent: this.agent,
          timeout: 120_000,
        },
        (res) => {
          const status = res.statusCode ?? 0;
          if (status < 200 || status >= 300) {
            // Non-success: drain the body, don't touch the file (caller cleans up).
            res.resume();
            res.on("end", () => {
              if (!settled) {
                settled = true;
                resolve(status);
              }
            });
            res.on("error", fail);
            return;
          }
          const out = createWriteStream(destPath);
          out.on("error", fail);
          res.on("error", fail);
          res.pipe(out);
          out.on("finish", () => {
            if (!settled) {
              settled = true;
              resolve(status);
            }
          });
        },
      );
      req.on("error", fail);
      req.on("timeout", () => req.destroy(new Error("request timed out")));
      req.on("socket", (s) => {
        s.on("error", (e: Error) => fail(e));
      });
      req.end();
    });
  }

  /** True if the object exists (cheap HEAD). */
  async has(key: string): Promise<boolean> {
    const res = await this.send("HEAD", key);
    if (res.status === 404) return false;
    if (res.status >= 200 && res.status < 300) return true;
    throw httpError("HEAD", key, res);
  }

  /** The object's ETag (unquoted, lowercased), or null if it doesn't exist. */
  async headETag(key: string): Promise<string | null> {
    const res = await this.send("HEAD", key);
    if (res.status === 404) return null;
    if (res.status < 200 || res.status >= 300) throw httpError("HEAD", key, res);
    const raw = res.headers["etag"];
    const etag = Array.isArray(raw) ? raw[0] : raw;
    return etag ? etag.replace(/"/g, "").toLowerCase() : null;
  }

  /**
   * PUT only when the remote object is absent or differs. R2's ETag for a
   * single-part PUT is the body's MD5, so we compare that without downloading.
   * Used for the mutable sparse-index entries so a shared cache refreshes an
   * entry when a new crate version republishes it, but skips unchanged ones.
   * Returns true if it uploaded.
   */
  async putIfChanged(key: string, body: Buffer, contentType?: string): Promise<boolean> {
    const remote = await this.headETag(key);
    const localMd5 = createHash("md5").update(body).digest("hex");
    if (remote === localMd5) return false;
    await this.put(key, body, contentType);
    return true;
  }

  async put(key: string, body: Buffer | Uint8Array, contentType?: string): Promise<void> {
    const buf = Buffer.isBuffer(body) ? body : Buffer.from(body);
    const res = await this.send("PUT", key, buf, contentType);
    if (res.status < 200 || res.status >= 300) throw httpError("PUT", key, res);
  }

  /** Delete an object. Used for test cleanup; a missing object is not an error. */
  async del(key: string): Promise<void> {
    const res = await this.send("DELETE", key);
    // S3/R2 return 204 on delete, 404 if already gone — both are fine.
    if (res.status !== 404 && (res.status < 200 || res.status >= 300)) {
      throw httpError("DELETE", key, res);
    }
  }

  // ---- Multipart upload (for objects too big to hash/buffer in one shot) ----

  /**
   * Upload a file to `key`, transparently using S3 multipart upload when it's
   * larger than the threshold. Parts are read from disk one at a time and sent
   * via presigned URLs (UNSIGNED-PAYLOAD), so we never hold the whole object in
   * memory nor hash >2GB at once (which throws "data is too long").
   */
  async putLargeFile(
    key: string,
    filePath: string,
    contentType?: string,
    // partSize/threshold are overridable only so tests can force the multipart
    // path on a small object; production uses the 64 MiB defaults.
    opts?: { partSize?: number; threshold?: number },
  ): Promise<void> {
    const partSize = opts?.partSize ?? MULTIPART_PART_SIZE;
    const threshold = opts?.threshold ?? MULTIPART_THRESHOLD;
    const { size } = await fsp.stat(filePath);
    if (size <= threshold) {
      await this.put(key, await fsp.readFile(filePath), contentType);
      return;
    }

    const uploadId = await this.createMultipart(key, contentType);
    try {
      const numParts = Math.ceil(size / partSize);
      const indices = Array.from({ length: numParts }, (_, i) => i);
      const parts = await mapPool(indices, MULTIPART_CONCURRENCY, async (i) => {
        const start = i * partSize;
        const len = Math.min(partSize, size - start);
        const body = await readFileChunk(filePath, start, len);
        const etag = await this.uploadPart(key, uploadId, i + 1, body);
        return { partNumber: i + 1, etag };
      });
      parts.sort((a, b) => a.partNumber - b.partNumber);
      await this.completeMultipart(key, uploadId, parts);
    } catch (e) {
      // Best effort: don't leave a dangling multipart upload accruing storage.
      await this.abortMultipart(key, uploadId).catch(() => {});
      throw e;
    }
  }

  private async createMultipart(key: string, contentType?: string): Promise<string> {
    const url = this.objectUrl(key);
    url.searchParams.set("uploads", "");
    const res = await this.sendUrl("POST", url, undefined, contentType);
    if (res.status < 200 || res.status >= 300) throw httpError("POST uploads", key, res);
    const m = res.body.toString("utf8").match(/<UploadId>([^<]+)<\/UploadId>/);
    if (!m) {
      throw new Error(`R2 multipart init for ${key} returned no UploadId: ${res.body.subarray(0, 300)}`);
    }
    return m[1]!;
  }

  /** Upload one part via a presigned PUT URL (body is not hashed). Returns ETag. */
  private async uploadPart(
    key: string,
    uploadId: string,
    partNumber: number,
    body: Buffer,
  ): Promise<string> {
    const url = this.objectUrl(key);
    url.searchParams.set("partNumber", String(partNumber));
    url.searchParams.set("uploadId", uploadId);
    const { path } = presign({
      method: "PUT",
      url,
      region: this.region,
      accessKeyId: this.accessKeyId,
      secretAccessKey: this.secretAccessKey,
      expiresSeconds: 3600,
    });
    const res = await this.sendPresigned("PUT", path, body);
    if (res.status < 200 || res.status >= 300) {
      throw httpError("PUT part", `${key}#${partNumber}`, res);
    }
    const raw = res.headers["etag"];
    const etag = Array.isArray(raw) ? raw[0] : raw;
    if (!etag) throw new Error(`R2 part ${partNumber} of ${key} returned no ETag`);
    return etag;
  }

  private async completeMultipart(
    key: string,
    uploadId: string,
    parts: Array<{ partNumber: number; etag: string }>,
  ): Promise<void> {
    const url = this.objectUrl(key);
    url.searchParams.set("uploadId", uploadId);
    const xml =
      "<CompleteMultipartUpload>" +
      parts
        .map((p) => `<Part><PartNumber>${p.partNumber}</PartNumber><ETag>${p.etag}</ETag></Part>`)
        .join("") +
      "</CompleteMultipartUpload>";
    const res = await this.sendUrl("POST", url, Buffer.from(xml), "application/xml");
    // S3/R2 can answer 200 with an <Error> body if completion fails server-side.
    if (res.status < 200 || res.status >= 300 || res.body.toString("utf8").includes("<Error>")) {
      throw httpError("POST complete", key, res);
    }
  }

  private async abortMultipart(key: string, uploadId: string): Promise<void> {
    const url = this.objectUrl(key);
    url.searchParams.set("uploadId", uploadId);
    await this.sendUrl("DELETE", url);
  }

  /** Send a request to an already-signed presigned target (no auth headers). */
  private async sendPresigned(method: "PUT" | "GET", path: string, body?: Buffer): Promise<RawResponse> {
    const host = this.endpoint.hostname;
    const port = this.endpoint.port;
    let lastErr: unknown;
    for (let attempt = 0; attempt < 4; attempt++) {
      try {
        const res = await this.exec(method, host, port, path, {}, body);
        if (res.status >= 500 && attempt < 3) {
          lastErr = new Error(`HTTP ${res.status}`);
          await delay(attempt);
          continue;
        }
        return res;
      } catch (e) {
        lastErr = e;
        if (attempt < 3) await delay(attempt);
      }
    }
    throw lastErr instanceof Error ? lastErr : new Error(String(lastErr));
  }
}

/** Read `len` bytes from `filePath` starting at `offset` into a fresh Buffer. */
async function readFileChunk(filePath: string, offset: number, len: number): Promise<Buffer> {
  const fh = await fsp.open(filePath, "r");
  try {
    const buf = Buffer.allocUnsafe(len);
    let read = 0;
    while (read < len) {
      const { bytesRead } = await fh.read(buf, read, len - read, offset + read);
      if (bytesRead === 0) break;
      read += bytesRead;
    }
    return read === len ? buf : buf.subarray(0, read);
  } finally {
    await fh.close();
  }
}

function httpError(method: string, key: string, res: RawResponse): Error {
  const snippet = res.body.subarray(0, 512).toString("utf8");
  return new Error(`R2 ${method} ${key} failed: HTTP ${res.status} ${snippet}`);
}
