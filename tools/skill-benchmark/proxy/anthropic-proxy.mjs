import { timingSafeEqual } from "node:crypto";
import { chmodSync, writeFileSync } from "node:fs";
import http from "node:http";
import https from "node:https";
import { pathToFileURL } from "node:url";

const DEFAULT_UPSTREAM = new URL("https://api.anthropic.com");
const ALLOWED_PATHS = new Set(["/v1/messages", "/v1/messages/count_tokens"]);
const REQUEST_HEADERS = new Set([
  "accept",
  "accept-encoding",
  "anthropic-beta",
  "anthropic-version",
  "content-type",
  "user-agent",
]);
const RESPONSE_HEADERS_TO_DROP = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "set-cookie",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]);

function required(name) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required`);
  }
  return value;
}

function positiveInteger(value, fallback, name) {
  if (value === undefined) {
    return fallback;
  }
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return parsed;
}

function tokenMatches(actual, expected) {
  if (!actual) {
    return false;
  }
  const actualBytes = Buffer.from(actual);
  const expectedBytes = Buffer.from(expected);
  return actualBytes.length === expectedBytes.length
    && timingSafeEqual(actualBytes, expectedBytes);
}

function requestToken(headers) {
  const authorization = headers.authorization;
  if (typeof authorization === "string" && authorization.startsWith("Bearer ")) {
    return authorization.slice("Bearer ".length);
  }
  return typeof headers["x-api-key"] === "string" ? headers["x-api-key"] : undefined;
}

function sendJson(response, status, body) {
  const encoded = Buffer.from(JSON.stringify(body));
  response.writeHead(status, {
    "content-length": encoded.length,
    "content-type": "application/json",
  });
  response.end(encoded);
}

async function readBody(request, maxBodyBytes) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > maxBodyBytes) {
      throw new Error("request body is too large");
    }
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}

function requestHeaders(headers, apiKey, bodyLength) {
  const forwarded = {
    "content-length": bodyLength,
    "x-api-key": apiKey,
  };
  for (const [name, value] of Object.entries(headers)) {
    if (REQUEST_HEADERS.has(name) && value !== undefined) {
      forwarded[name] = value;
    }
  }
  return forwarded;
}

function responseHeaders(headers) {
  return Object.fromEntries(
    Object.entries(headers).filter(([name]) => !RESPONSE_HEADERS_TO_DROP.has(name)),
  );
}

export function createAnthropicProxy({
  apiKey,
  token,
  allowedModels,
  maxRequests = 200,
  maxBodyBytes = 16 * 1024 * 1024,
  upstream = DEFAULT_UPSTREAM,
}) {
  if (!apiKey || !token || allowedModels.size === 0) {
    throw new Error("apiKey, token, and allowedModels are required");
  }
  let requests = 0;
  const transport = upstream.protocol === "https:" ? https : http;

  return http.createServer(async (request, response) => {
    const url = new URL(request.url ?? "/", "http://localhost");
    if (request.method === "GET" && url.pathname === "/healthz") {
      sendJson(response, 200, { status: "ok" });
      return;
    }
    if (!tokenMatches(requestToken(request.headers), token)) {
      sendJson(response, 401, { error: { type: "authentication_error", message: "unauthorized" } });
      return;
    }
    if (request.method !== "POST" || !ALLOWED_PATHS.has(url.pathname)) {
      sendJson(response, 404, { error: { type: "invalid_request_error", message: "endpoint not allowed" } });
      return;
    }
    if (requests >= maxRequests) {
      sendJson(response, 429, { error: { type: "rate_limit_error", message: "request limit reached" } });
      return;
    }

    let body;
    let parsed;
    try {
      body = await readBody(request, maxBodyBytes);
      parsed = JSON.parse(body.toString("utf8"));
    } catch (error) {
      sendJson(response, 400, {
        error: { type: "invalid_request_error", message: error.message },
      });
      return;
    }
    if (typeof parsed.model !== "string" || !allowedModels.has(parsed.model)) {
      sendJson(response, 400, {
        error: { type: "invalid_request_error", message: "model not allowed" },
      });
      return;
    }
    if (
      url.pathname === "/v1/messages"
      && (!Number.isSafeInteger(parsed.max_tokens) || parsed.max_tokens <= 0 || parsed.max_tokens > 65536)
    ) {
      sendJson(response, 400, {
        error: { type: "invalid_request_error", message: "max_tokens not allowed" },
      });
      return;
    }

    requests += 1;
    const requestNumber = requests;
    const upstreamRequest = transport.request(
      new URL(`${url.pathname}${url.search}`, upstream),
      {
        method: "POST",
        headers: requestHeaders(request.headers, apiKey, body.length),
        timeout: 10 * 60 * 1000,
      },
      upstreamResponse => {
        console.log(
          `request=${requestNumber} method=POST path=${url.pathname} model=${parsed.model} status=${upstreamResponse.statusCode ?? 502}`,
        );
        response.writeHead(
          upstreamResponse.statusCode ?? 502,
          responseHeaders(upstreamResponse.headers),
        );
        upstreamResponse.pipe(response);
      },
    );
    upstreamRequest.on("timeout", () => upstreamRequest.destroy(new Error("upstream timed out")));
    upstreamRequest.on("error", error => {
      console.error(
        `request=${requestNumber} method=POST path=${url.pathname} model=${parsed.model} status=upstream_error`,
      );
      if (!response.headersSent) {
        sendJson(response, 502, {
          error: { type: "api_error", message: `upstream request failed: ${error.message}` },
        });
      } else {
        response.destroy(error);
      }
    });
    request.on("aborted", () => upstreamRequest.destroy());
    upstreamRequest.end(body);
  });
}

function start() {
  const infoPath = required("SKILL_BENCH_PROXY_INFO_PATH");
  const lifetimeMs = positiveInteger(
    process.env.SKILL_BENCH_PROXY_LIFETIME_MS,
    45 * 60 * 1000,
    "SKILL_BENCH_PROXY_LIFETIME_MS",
  );
  const server = createAnthropicProxy({
    apiKey: required("ANTHROPIC_API_KEY"),
    token: required("SKILL_BENCH_PROXY_TOKEN"),
    allowedModels: new Set(required("SKILL_BENCH_PROXY_ALLOWED_MODELS").split(",")),
    maxRequests: positiveInteger(
      process.env.SKILL_BENCH_PROXY_MAX_REQUESTS,
      200,
      "SKILL_BENCH_PROXY_MAX_REQUESTS",
    ),
    maxBodyBytes: positiveInteger(
      process.env.SKILL_BENCH_PROXY_MAX_BODY_BYTES,
      16 * 1024 * 1024,
      "SKILL_BENCH_PROXY_MAX_BODY_BYTES",
    ),
  });

  const stop = () => server.close(() => process.exit(0));
  process.on("SIGINT", stop);
  process.on("SIGTERM", stop);
  setTimeout(() => {
    server.closeAllConnections();
    server.close(() => process.exit(0));
  }, lifetimeMs).unref();

  server.listen(0, "127.0.0.1", () => {
    const address = server.address();
    if (typeof address === "string" || address === null) {
      throw new Error("proxy did not bind to a TCP port");
    }
    writeFileSync(infoPath, JSON.stringify({ pid: process.pid, port: address.port }));
    chmodSync(infoPath, 0o644);
    console.log(`Anthropic proxy listening on 127.0.0.1:${address.port}`);
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  start();
}
