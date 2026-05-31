import { createHash, createHmac } from "node:crypto";

/**
 * Minimal AWS Signature Version 4 signer for S3 ("s3" service), enough to sign
 * the GET/PUT/HEAD/DELETE/POST requests this action makes against R2, plus
 * query-string *presigning* for multipart part uploads.
 *
 * R2's S3 API requires SigV4 (it rejects bearer-token auth), and signing it
 * ourselves lets us talk to R2 over node's built-in `https` instead of pulling
 * the ~1MB AWS SDK into the committed bundle. This mirrors the `curl --aws-sigv4`
 * approach the previous shell action used.
 */

export interface SignInput {
  method: "GET" | "PUT" | "HEAD" | "DELETE" | "POST";
  /** Full URL, e.g. https://<acct>.r2.cloudflarestorage.com/<bucket>/<key> */
  url: URL;
  region: string;
  accessKeyId: string;
  secretAccessKey: string;
  /** Request body (empty for GET/HEAD/DELETE). */
  body?: Buffer;
  /** Extra headers to include in the signature (e.g. content-type). */
  headers?: Record<string, string>;
  /** ISO-ish timestamp `YYYYMMDDTHHMMSSZ`; defaults to now. Injectable for tests. */
  amzDate?: string;
}

const SERVICE = "s3";
const UNSIGNED_PAYLOAD = "UNSIGNED-PAYLOAD";

function sha256Hex(data: Buffer | string): string {
  return createHash("sha256").update(data).digest("hex");
}

function hmac(key: Buffer | string, data: string): Buffer {
  return createHmac("sha256", key).update(data, "utf8").digest();
}

/**
 * AWS URI-encodes each path segment but keeps the `/` separators. Both the
 * signature and the on-the-wire request path must use this — otherwise a key
 * with a reserved char (e.g. the `+` in `foo-1.0.0+build.crate`) is signed as
 * `%2B` but sent as `+`, which R2 rejects with `403 SignatureDoesNotMatch`.
 */
export function encodePath(pathname: string): string {
  return pathname
    .split("/")
    .map((seg) => encodeRfc3986(decodeURIComponent(seg)))
    .join("/");
}

/** RFC 3986 encoding: encodeURIComponent plus the characters it leaves alone. */
function encodeRfc3986(str: string): string {
  return encodeURIComponent(str).replace(
    /[!'()*]/g,
    (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase(),
  );
}

/**
 * AWS canonical query string: sort by key, RFC3986-encode key and value. Used
 * for both signing and the wire request, so encoding can never drift (URL's own
 * search serialization differs from AWS's, which would break signed queries
 * like `?uploadId=...` whose value can contain `/`, `+` or `=`).
 */
export function canonicalQuery(params: URLSearchParams): string {
  return [...params.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([k, v]) => `${encodeRfc3986(k)}=${encodeRfc3986(v)}`)
    .join("&");
}

function amzNow(): string {
  // YYYYMMDDTHHMMSSZ — argless `new Date()` is fine in the bundled action; tests
  // pass an explicit amzDate so they don't depend on the clock.
  return new Date().toISOString().replace(/[:-]|\.\d{3}/g, "");
}

function signingKey(secretAccessKey: string, dateStamp: string, region: string): Buffer {
  const kDate = hmac(`AWS4${secretAccessKey}`, dateStamp);
  const kRegion = hmac(kDate, region);
  const kService = hmac(kRegion, SERVICE);
  return hmac(kService, "aws4_request");
}

/**
 * Returns the headers (including Authorization) to send with the request.
 * The caller supplies the same method/url/body to the HTTP client.
 */
export function signRequest(input: SignInput): Record<string, string> {
  const { method, url, region, accessKeyId, secretAccessKey } = input;
  const body = input.body ?? Buffer.alloc(0);
  const amzDate = input.amzDate ?? amzNow();
  const dateStamp = amzDate.slice(0, 8);
  const payloadHash = sha256Hex(body);

  // Assemble headers to sign. Host + x-amz-date + x-amz-content-sha256 are
  // always signed; any extra headers (e.g. content-type) join them.
  const headers: Record<string, string> = {
    host: url.host,
    "x-amz-content-sha256": payloadHash,
    "x-amz-date": amzDate,
  };
  for (const [k, v] of Object.entries(input.headers ?? {})) {
    headers[k.toLowerCase()] = v;
  }

  const signedHeaderNames = Object.keys(headers).sort();
  const canonicalHeaders =
    signedHeaderNames.map((h) => `${h}:${headers[h]!.trim()}\n`).join("");
  const signedHeaders = signedHeaderNames.join(";");

  const canonicalRequest = [
    method,
    encodePath(url.pathname),
    canonicalQuery(url.searchParams),
    canonicalHeaders,
    signedHeaders,
    payloadHash,
  ].join("\n");

  const scope = `${dateStamp}/${region}/${SERVICE}/aws4_request`;
  const stringToSign = [
    "AWS4-HMAC-SHA256",
    amzDate,
    scope,
    sha256Hex(canonicalRequest),
  ].join("\n");

  const kSigning = signingKey(secretAccessKey, dateStamp, region);
  const signature = createHmac("sha256", kSigning).update(stringToSign, "utf8").digest("hex");

  const authorization =
    `AWS4-HMAC-SHA256 Credential=${accessKeyId}/${scope}, ` +
    `SignedHeaders=${signedHeaders}, Signature=${signature}`;

  return { ...headers, authorization };
}

export interface PresignInput {
  method: "PUT" | "GET";
  url: URL;
  region: string;
  accessKeyId: string;
  secretAccessKey: string;
  /** URL validity in seconds. */
  expiresSeconds: number;
  amzDate?: string;
}

/**
 * Build a SigV4 *query-string* presigned request. The payload is signed as
 * `UNSIGNED-PAYLOAD`, so the (potentially multi-GB) body is never hashed — this
 * is what lets us stream huge multipart parts to R2 without OpenSSL's ~2GB
 * `Hash.update` limit ("data is too long"). Only the `host` header is signed,
 * so the caller sends the body with no auth/x-amz headers at all.
 *
 * Returns the request-target (`<encoded-path>?<query>`) and the host to connect
 * to; both come straight from the signed canonical request so they can't drift.
 */
export function presign(input: PresignInput): { path: string; host: string } {
  const { method, url, region, accessKeyId, secretAccessKey, expiresSeconds } = input;
  const amzDate = input.amzDate ?? amzNow();
  const dateStamp = amzDate.slice(0, 8);
  const scope = `${dateStamp}/${region}/${SERVICE}/aws4_request`;

  // Start from any query already on the URL (e.g. partNumber / uploadId) and add
  // the X-Amz-* auth params (everything except the signature itself).
  const params = new URLSearchParams(url.searchParams);
  params.set("X-Amz-Algorithm", "AWS4-HMAC-SHA256");
  params.set("X-Amz-Credential", `${accessKeyId}/${scope}`);
  params.set("X-Amz-Date", amzDate);
  params.set("X-Amz-Expires", String(expiresSeconds));
  params.set("X-Amz-SignedHeaders", "host");

  const canonicalRequest = [
    method,
    encodePath(url.pathname),
    canonicalQuery(params),
    `host:${url.host}\n`,
    "host",
    UNSIGNED_PAYLOAD,
  ].join("\n");

  const stringToSign = [
    "AWS4-HMAC-SHA256",
    amzDate,
    scope,
    sha256Hex(canonicalRequest),
  ].join("\n");

  const kSigning = signingKey(secretAccessKey, dateStamp, region);
  const signature = createHmac("sha256", kSigning).update(stringToSign, "utf8").digest("hex");

  const query = `${canonicalQuery(params)}&X-Amz-Signature=${signature}`;
  return { path: `${encodePath(url.pathname)}?${query}`, host: url.host };
}
