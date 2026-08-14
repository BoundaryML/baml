import 'dotenv/config';
import assert from 'node:assert';
import { pathToFileURL } from 'node:url';
import cors from 'cors';
import express from 'express';
import { createProxyMiddleware } from 'http-proxy-middleware';

// From https://nodejs.org/api/url.html#url-strings-and-url-objects:
// ┌────────────────────────────────────────────────────────────────────────────────────────────────┐
// │                                              href                                              │
// ├──────────┬──┬─────────────────────┬────────────────────────┬───────────────────────────┬───────┤
// │ protocol │  │        auth         │          host          │           path            │ hash  │
// │          │  │                     ├─────────────────┬──────┼──────────┬────────────────┤       │
// │          │  │                     │    hostname     │ port │ pathname │     search     │       │
// │          │  │                     │                 │      │          ├─┬──────────────┤       │
// │          │  │                     │                 │      │          │ │    query     │       │
// "  https:   //    user   :   pass   @ sub.example.com : 8080   /p/a/t/h  ?  query=string   #hash "
// │          │  │          │          │    hostname     │ port │          │                │       │
// │          │  │          │          ├─────────────────┴──────┤          │                │       │
// │ protocol │  │ username │ password │          host          │          │                │       │
// ├──────────┴──┼──────────┴──────────┼────────────────────────┤          │                │       │
// │   origin    │                     │         origin         │ pathname │     search     │ hash  │
// ├─────────────┴─────────────────────┴────────────────────────┴──────────┴────────────────┴───────┤
// │                                              href                                              │
// └────────────────────────────────────────────────────────────────────────────────────────────────┘

// These are the origins which we may "leak" our API keys to.
//
// We inject our API keys into requests to these domains so that promptfiddle users are not
// required to provide their own API keys, but we must make sure that these API keys cannot be
// leaked to third parties.
//
// Since all we do is blindly proxy requests from the WASM runtime, and promptfiddle users may
// override the base_url of any client, this allowlist guarantees that we only inject API keys
// in requests to these model providers.
export function buildApiKeyInjectionAllowed(env = process.env) {
  // [provider origin, header name, env var, value formatter]
  const providers = [
    ['https://api.openai.com', 'Authorization', 'OPENAI_API_KEY', (k) => `Bearer ${k}`],
    ['https://api.anthropic.com', 'x-api-key', 'ANTHROPIC_API_KEY', (k) => k],
    ['https://ai-gateway.vercel.sh', 'Authorization', 'AI_GATEWAY_API_KEY', (k) => `Bearer ${k}`],
    //['https://generativelanguage.googleapis.com', 'x-goog-api-key', 'GOOGLE_API_KEY', (k) => k],
    //['https://openrouter.ai', 'Authorization', 'OPENROUTER_API_KEY', (k) => `Bearer ${k}`],
    //['https://api.llmapi.com', 'Authorization', 'LLAMA_API_KEY', (k) => `Bearer ${k}`],
  ];

  const allowed = {};
  for (const [url, header, envVar, format] of providers) {
    assert(
      url === new URL(url).origin && new URL(url).protocol === 'https:',
      `Keys of API_KEY_INJECTION_ALLOWED must be HTTPS origins for model providers, got ${url}`,
    );
    // Fail fast at startup if a provider key is missing, rather than crashing
    // ClientRequest.setHeader (or injecting a bogus "Bearer undefined") per request.
    const key = env[envVar];
    assert(key, `Missing required environment variable ${envVar} (needed to inject keys for ${url})`);
    allowed[url] = { [header]: format(key) };
  }

  return allowed;
}

// Attached in Cloudflare with a Request Header Transform Rule.
// Ensures that everyone who hits this API has gone through Cloudflare rate limiting.
// Node lowercases header names but preserves underscores, so the incoming header
// lands under this lowercased key.
export const PROXY_TOKEN_HEADER = 'proxy_promptfiddle_com_token';

export function buildApp({
  apiKeyInjectionAllowed = buildApiKeyInjectionAllowed(),
  logger = console,
} = {}) {
  const app = express();

  app.use(cors());

  app.use((req, res, next) => {
    // The token must always be configured: a missing/empty value means the
    // Cloudflare transform isn't wired up, so we fail closed rather than serve
    // requests that bypassed rate limiting. With it guaranteed set, the header
    // check below is a plain comparison.
    const expectedToken = process.env.PROXY_PROMPTFIDDLE_COM_TOKEN;
    if (!expectedToken) {
      logger.error('PROXY_PROMPTFIDDLE_COM_TOKEN must be set, failing request');
      res.status(403).json({ error: 'forbidden' });
      return;
    }
    if (req.headers[PROXY_TOKEN_HEADER] !== expectedToken) {
      res.status(403).json({ error: 'forbidden' });
      return;
    }
    delete req.headers[PROXY_TOKEN_HEADER];
    next();
  });

  app.use(
    createProxyMiddleware({
      changeOrigin: true,
      followRedirects: true,
      pathRewrite: (path, req) => {
        // Ensure the URL does not end with a slash
        if (path.endsWith('/')) {
          return path.slice(0, -1);
        }
        return path;
      },
      router: (req) => {
        // Extract the original target URL from the custom header
        const originalUrl = req.headers['baml-original-url'];

        if (typeof originalUrl === 'string') {
          return originalUrl;
        } else {
          throw new Error('baml-original-url header is missing or invalid');
        }
      },
      logger,
      on: {
        proxyReq: (proxyReq, req, res) => {
          try {
            const bamlOriginalUrl = req.headers['baml-original-url'];
            if (bamlOriginalUrl === undefined) {
              return;
            }
            const proxyOrigin = new URL(bamlOriginalUrl).origin;
            // It is very important that we ONLY resolve against apiKeyInjectionAllowed
            // by using the URL origin! (i.e. NOT using str.startsWith - the latter can still
            // leak API keys to malicious subdomains e.g. https://api.openai.com.evil.com)
            const headers = apiKeyInjectionAllowed[proxyOrigin];
            if (headers === undefined) {
              return;
            }
            for (const [header, value] of Object.entries(headers)) {
              proxyReq.setHeader(header, value);
            }
            proxyReq.removeHeader('origin');
            logger.log(`Forwarding request for ${bamlOriginalUrl}`);
          } catch (err) {
            // This is not logger.warn because it's not important
            logger.log('baml-original-url is not parsable', err);
          }
        },
        proxyRes: (proxyRes, req, res) => {
          proxyRes.headers['Access-Control-Allow-Origin'] = '*';
        },
        error: (error) => {
          logger.error('proxy error:', error);
        },
      },
    }),
  );

  return app;
}

// Start the web server on port 3000 only when this file is run directly
// (not when imported by tests).
const isMain = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  buildApp().listen(3000, () => {
    console.log('Server is listening on port 3000');
  });
}
