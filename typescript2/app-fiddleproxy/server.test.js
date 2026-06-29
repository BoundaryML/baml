import http from 'node:http';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { buildApiKeyInjectionAllowed, buildApp, PROXY_TOKEN_HEADER } from './server.js';

// Keep http-proxy-middleware quiet during tests.
const quietLogger = { log() {}, error() {}, warn() {}, info() {}, debug() {} };

// A full set of provider keys — buildApiKeyInjectionAllowed() now requires every
// key to be present, so tests that build the real allowlist pass all of them.
const ALL_KEYS = {
  OPENAI_API_KEY: 'sk-openai',
  ANTHROPIC_API_KEY: 'sk-anthropic',
  AI_GATEWAY_API_KEY: 'sk-ai-gateway',
  GOOGLE_API_KEY: 'k-google',
  OPENROUTER_API_KEY: 'sk-openrouter',
  LLAMA_API_KEY: 'sk-llama',
};

// Listen on an ephemeral port and resolve with the base URL.
function listen(server) {
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve(`http://127.0.0.1:${port}`);
    });
  });
}

function close(server) {
  return new Promise((resolve) => server.close(resolve));
}

// A fake upstream "model provider" that records every request it receives.
function createUpstream() {
  const received = [];
  const server = http.createServer((req, res) => {
    received.push({ method: req.method, url: req.url, headers: { ...req.headers } });
    res.writeHead(200, { 'content-type': 'application/json' });
    res.end(JSON.stringify({ ok: true }));
  });
  return { server, received };
}

// Snapshot/restore the env vars these tests mutate so we don't clobber a real
// .env that dotenv may have loaded when server.js was imported.
const ENV_KEYS = ['PROXY_PROMPTFIDDLE_COM_TOKEN', 'OPENAI_API_KEY'];
let savedEnv;
beforeEach(() => {
  savedEnv = {};
  for (const key of ENV_KEYS) savedEnv[key] = process.env[key];
});
afterEach(() => {
  for (const key of ENV_KEYS) {
    if (savedEnv[key] === undefined) delete process.env[key];
    else process.env[key] = savedEnv[key];
  }
});

describe('PROXY_PROMPTFIDDLE_COM_TOKEN', () => {
  let appServer;
  let appUrl;
  let upstream;
  let upstreamUrl;

  beforeEach(async () => {
    process.env.PROXY_PROMPTFIDDLE_COM_TOKEN = 'shh-secret';

    upstream = createUpstream();
    upstreamUrl = await listen(upstream.server);

    // These tests only exercise the token gate; an empty allowlist avoids needing
    // provider keys in the environment.
    appServer = http.createServer(buildApp({ logger: quietLogger, apiKeyInjectionAllowed: {} }));
    appUrl = await listen(appServer);
  });

  afterEach(async () => {
    await close(appServer);
    await close(upstream.server);
  });

  it('rejects requests with no token header', async () => {
    const res = await fetch(`${appUrl}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'baml-original-url': upstreamUrl },
    });

    expect(res.status).toBe(403);
    expect(upstream.received).toHaveLength(0); // upstream never hit
  });

  it('rejects requests whose token does not match', async () => {
    const res = await fetch(`${appUrl}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'baml-original-url': upstreamUrl, [PROXY_TOKEN_HEADER]: 'wrong-token' },
    });

    expect(res.status).toBe(403);
    expect(upstream.received).toHaveLength(0);
  });

  it('rejects requests when the token env var is set but empty', async () => {
    process.env.PROXY_PROMPTFIDDLE_COM_TOKEN = '';

    const res = await fetch(`${appUrl}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'baml-original-url': upstreamUrl, [PROXY_TOKEN_HEADER]: 'anything' },
    });

    expect(res.status).toBe(403);
    expect(upstream.received).toHaveLength(0);
  });

  it('forwards a request with the correct token and strips the token before forwarding', async () => {
    const res = await fetch(`${appUrl}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'baml-original-url': upstreamUrl, [PROXY_TOKEN_HEADER]: 'shh-secret' },
    });

    expect(res.status).toBe(200);
    expect(upstream.received).toHaveLength(1);
    // The token must never reach the upstream provider.
    expect(upstream.received[0].headers[PROXY_TOKEN_HEADER]).toBeUndefined();
  });
});

describe('API key injection', () => {
  it('maps each provider origin to its API key header', () => {
    const allowed = buildApiKeyInjectionAllowed(ALL_KEYS);
    expect(allowed['https://api.openai.com']).toEqual({ Authorization: 'Bearer sk-openai' });
    expect(allowed['https://api.anthropic.com']).toEqual({ 'x-api-key': 'sk-anthropic' });
  });

  it('throws when a required provider key is missing', () => {
    // Fail fast at startup rather than crashing setHeader per request.
    expect(() => buildApiKeyInjectionAllowed({})).toThrow();
    expect(() => buildApiKeyInjectionAllowed({ ...ALL_KEYS, ANTHROPIC_API_KEY: undefined })).toThrow(
      /ANTHROPIC_API_KEY/,
    );
  });

  it('attaches the API key header to the upstream request', async () => {
    // The token gate is always on; pass the matching token through.
    process.env.PROXY_PROMPTFIDDLE_COM_TOKEN = 'shh-secret';

    const upstream = createUpstream();
    const upstreamUrl = await listen(upstream.server);
    const upstreamOrigin = new URL(upstreamUrl).origin;

    // Point the allowlist at our fake upstream, using the exact headers the real
    // openai config produces.
    const openaiHeaders = buildApiKeyInjectionAllowed(ALL_KEYS)['https://api.openai.com'];
    const app = buildApp({
      logger: quietLogger,
      apiKeyInjectionAllowed: { [upstreamOrigin]: openaiHeaders },
    });
    const appServer = http.createServer(app);
    const appUrl = await listen(appServer);

    try {
      const res = await fetch(`${appUrl}/v1/chat/completions`, {
        method: 'POST',
        headers: { 'baml-original-url': upstreamUrl, [PROXY_TOKEN_HEADER]: 'shh-secret' },
      });

      expect(res.status).toBe(200);
      expect(upstream.received).toHaveLength(1);
      expect(upstream.received[0].headers['authorization']).toBe('Bearer sk-openai');
    } finally {
      await close(appServer);
      await close(upstream.server);
    }
  });

  it('does NOT attach API keys for an origin that is not on the allowlist', async () => {
    // The token gate is always on; pass the matching token through.
    process.env.PROXY_PROMPTFIDDLE_COM_TOKEN = 'shh-secret';

    const upstream = createUpstream();
    const upstreamUrl = await listen(upstream.server);

    // The real allowlist only knows real provider origins, so our localhost
    // upstream gets no injected key.
    const app = buildApp({
      logger: quietLogger,
      apiKeyInjectionAllowed: buildApiKeyInjectionAllowed(ALL_KEYS),
    });
    const appServer = http.createServer(app);
    const appUrl = await listen(appServer);

    try {
      const res = await fetch(`${appUrl}/v1/chat/completions`, {
        method: 'POST',
        headers: { 'baml-original-url': upstreamUrl, [PROXY_TOKEN_HEADER]: 'shh-secret' },
      });

      expect(res.status).toBe(200);
      expect(upstream.received).toHaveLength(1);
      expect(upstream.received[0].headers['authorization']).toBeUndefined();
    } finally {
      await close(appServer);
      await close(upstream.server);
    }
  });
});
