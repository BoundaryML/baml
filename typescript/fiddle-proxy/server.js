const cors = require('cors')
const { createProxyMiddleware } = require('http-proxy-middleware')
const assert = require('assert')
const app = require('express')()
require('dotenv').config()

app.use(cors())

// These are the domains which we may "leak" our API keys to.
//
// We inject our API keys into requests to these domains so that promptfiddle users are not
// required to provide their own API keys, but we must make sure that these API keys cannot be
// leaked to third parties.
//
// Since all we do is blindly proxy requests from the WASM runtime, and promptfiddle users may
// override the base_url of any client, this allowlist guarantees that we only inject API keys
// in requests to these model providers.
const API_KEY_INJECTION_ALLOWED = {
  'https://api.openai.com/': { Authorization: `Bearer ${process.env.OPENAI_API_KEY}` },
  'https://api.anthropic.com/': { 'x-api-key': process.env.ANTHROPIC_API_KEY },
  'https://generativelanguage.googleapis.com/': { 'x-goog-api-key': process.env.GOOGLE_API_KEY },
}

// Consult sam@ before changing this.
for (const url of Object.keys(API_KEY_INJECTION_ALLOWED)) {
  assert(
    // The trailing slash is important! Otherwise, users could bypass the allowlist using
    // a subdomain like https://api.openai.com.evil.com/
    url === `https://${new URL(url).hostname}/`,
    `Keys of API_KEY_INJECTION_ALLOWED must be root HTTPS URLs for model providers, got ${url}`,
  )
}

app.use(
  createProxyMiddleware({
    changeOrigin: true,
    pathRewrite: (path, req) => {
      // Ensure the URL does not end with a slash
      if (path.endsWith('/')) {
        return path.slice(0, -1)
      }
      return path
    },
    router: (req) => {
      // Extract the original target URL from the custom header
      const originalUrl = req.headers['baml-original-url']

      if (typeof originalUrl === 'string') {
        return originalUrl
      } else {
        throw new Error('baml-original-url header is missing or invalid')
      }
    },
    logger: console,
    on: {
      proxyReq: (proxyReq, req, res) => {
        for (const [url, headers] of Object.entries(API_KEY_INJECTION_ALLOWED)) {
          if (req.headers['baml-original-url'] == url) {
            for (const [header, value] of Object.entries(headers)) {
              proxyReq.setHeader(header, value)
            }
          }
        }
      },
      proxyRes: (proxyRes, req, res) => {
        proxyRes.headers['Access-Control-Allow-Origin'] = '*'
      },
      error: (error) => {
        console.error('proxy error:', error)
      },
    },
  }),
)

// Start web server on port 3000
app.listen(3000, () => {
  console.log('Server is listening on port 3000')
})
