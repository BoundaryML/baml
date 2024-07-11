const cors = require('cors')
const { createProxyMiddleware } = require('http-proxy-middleware')
const assert = require('assert')
const app = require('express')()
require('dotenv').config()

app.use(cors())

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
const API_KEY_INJECTION_ALLOWED = {
  'https://api.openai.com': { Authorization: `Bearer ${process.env.OPENAI_API_KEY}` },
  'https://api.anthropic.com': { 'x-api-key': process.env.ANTHROPIC_API_KEY },
  'https://generativelanguage.googleapis.com': { 'x-goog-api-key': process.env.GOOGLE_API_KEY },
}

// Consult sam@ before changing this.
for (const url of Object.keys(API_KEY_INJECTION_ALLOWED)) {
  assert(
    url === new URL(url).origin && new URL(url).protocol === 'https:',
    `Keys of API_KEY_INJECTION_ALLOWED must be HTTPS origins for model providers, got ${url}`,
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
        try {
          const bamlOriginalUrl = req.headers['baml-original-url']
          if (bamlOriginalUrl === undefined) {
            return
          }
          const proxyOrigin = new URL(bamlOriginalUrl).origin
          // It is very important that we ONLY resolve against API_KEY_INJECTION_ALLOWED
          // by using the URL origin! (i.e. NOT using str.startsWith - the latter can still
          // leak API keys to malicious subdomains e.g. https://api.openai.com.evil.com)
          const headers = API_KEY_INJECTION_ALLOWED[proxyOrigin]
          if (headers === undefined) {
            return
          }
          for (const [header, value] of Object.entries(headers)) {
            proxyReq.setHeader(header, value)
          }
        } catch (err) {
          // This is not console.warn because it's not important
          console.log('baml-original-url is not parsable', err)
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
