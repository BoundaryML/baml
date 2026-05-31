import { test } from "node:test";
import assert from "node:assert/strict";
import { signRequest, presign, encodePath, canonicalQuery } from "../src/sigv4.ts";

/**
 * Known-answer test using the canonical example credentials from the AWS SigV4
 * test suite (the "wikipedia"/docs example widely used to validate signers):
 * a GET on s3.amazonaws.com at a fixed timestamp must produce this exact
 * signature. If our canonicalization or HMAC chain drifts, this breaks.
 */
test("signRequest: matches the AWS SigV4 reference vector", () => {
  const headers = signRequest({
    method: "GET",
    url: new URL("https://examplebucket.s3.amazonaws.com/test.txt"),
    region: "us-east-1",
    accessKeyId: "AKIAIOSFODNN7EXAMPLE",
    secretAccessKey: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    amzDate: "20130524T000000Z",
    // The reference vector also signs a Range header; include it to exercise
    // extra-header canonicalization.
    headers: { range: "bytes=0-9" },
  });

  assert.equal(
    headers["authorization"],
    "AWS4-HMAC-SHA256 " +
      "Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, " +
      "SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, " +
      "Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41",
  );
  // Empty-body GET uses the well-known empty-payload hash.
  assert.equal(
    headers["x-amz-content-sha256"],
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
});

test("signRequest: signs content-type and encodes the key path", () => {
  const headers = signRequest({
    method: "PUT",
    url: new URL("https://acct.r2.cloudflarestorage.com/bucket/a%20b/c+d.crate"),
    region: "auto",
    accessKeyId: "AKID",
    secretAccessKey: "secret",
    amzDate: "20240101T000000Z",
    body: Buffer.from("hello"),
    headers: { "content-type": "application/x-tar" },
  });
  // content-type participates in the signed headers list (sorted, semicolon-joined).
  assert.match(headers["authorization"]!, /SignedHeaders=content-type;host;x-amz-content-sha256;x-amz-date/);
  // payload hash is of the actual body, not the empty hash.
  assert.notEqual(
    headers["x-amz-content-sha256"],
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
});

test("encodePath: encodes '+' as %2B but preserves '/' separators", () => {
  // The '+' in a crate filename must be percent-encoded, or the wire path won't
  // match the signed path and R2 returns 403 SignatureDoesNotMatch.
  assert.equal(
    encodePath("/bucket/crates/foo-1.0.0+build.crate"),
    "/bucket/crates/foo-1.0.0%2Bbuild.crate",
  );
});

test("canonicalQuery: sorts keys and RFC3986-encodes values (e.g. uploadId)", () => {
  const params = new URLSearchParams();
  params.set("uploadId", "ab/cd+ef=gh");
  params.set("partNumber", "2");
  // sorted: partNumber before uploadId; '/','+','=' all encoded in the value.
  assert.equal(canonicalQuery(params), "partNumber=2&uploadId=ab%2Fcd%2Bef%3Dgh");
});

test("presign: builds a query-signed URL over UNSIGNED-PAYLOAD", () => {
  const { path } = presign({
    method: "PUT",
    url: new URL("https://acct.r2.cloudflarestorage.com/bucket/git-db/x.tar?partNumber=1&uploadId=ABC"),
    region: "auto",
    accessKeyId: "AKID",
    secretAccessKey: "secret",
    expiresSeconds: 3600,
    amzDate: "20240101T000000Z",
  });
  // The original query params survive, the X-Amz-* auth params are added, and a
  // 64-hex signature is appended last. No auth header is needed by the caller.
  assert.match(path, /^\/bucket\/git-db\/x\.tar\?/);
  assert.match(path, /partNumber=1/);
  assert.match(path, /uploadId=ABC/);
  assert.match(path, /X-Amz-Algorithm=AWS4-HMAC-SHA256/);
  assert.match(path, /X-Amz-Credential=AKID%2F20240101%2Fauto%2Fs3%2Faws4_request/);
  assert.match(path, /X-Amz-SignedHeaders=host/);
  assert.match(path, /&X-Amz-Signature=[0-9a-f]{64}$/);
});
