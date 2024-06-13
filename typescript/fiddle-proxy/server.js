const cors = require('cors')
const { createProxyMiddleware } = require('http-proxy-middleware')
const app = require('express')()
require('dotenv').config()

app.use(cors())

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
        if (req.headers['baml-original-url'].includes('openai.com')) {
          if (process.env.OPENAI_API_KEY === undefined) {
            throw new Error('OPENAI_API_KEY is missing')
          }
          proxyReq.setHeader('Authorization', `Bearer ${process.env.OPENAI_API_KEY}`)
        }
        if (req.headers['baml-original-url'].includes('anthropic')) {
          if (process.env.ANTHROPIC_API_KEY === undefined) {
            throw new Error('ANTHROPIC_API_KEY is missing')
          }
          proxyReq.setHeader('x-api-key', process.env.ANTHROPIC_API_KEY)
        }
        if (req.headers['baml-original-url'].includes('gemini')) {
          // if (process.env.GOOGLE_API_KEY === undefined) {
          //   throw new Error('GOOGLE_API_KEY is missing')
          // }
          proxyReq.setHeader('x-goog-api-key', process.env.GOOGLE_API_KEY)
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
