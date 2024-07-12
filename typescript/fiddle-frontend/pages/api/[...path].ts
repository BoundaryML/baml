import httpProxy from 'http-proxy'
import { NextApiRequest, NextApiResponse } from 'next'

const API_URL = process.env.API_URL // The actual URL of your API

const proxy = httpProxy.createProxyServer()

// Make sure that we don't parse JSON bodies on this route:
export const config = {
  api: {
    bodyParser: false,
  },
}

export const proxyApi = (req: NextApiRequest, res: NextApiResponse): Promise<void> => {
  return new Promise((resolve, reject) => {
    const siteUrl = process.env.NEXT_PUBLIC_SITE_URL
    const originalUrlHeader = req.headers['baml-original-url'] as string
    if (!originalUrlHeader) {
      return reject(new Error('baml-original-url header is missing'))
    }
    // req.headers['origin'] = siteUrl
    req.url = req?.url?.replace(/^\/api/, '')
    console.log('originalUrlHeader', originalUrlHeader)

    proxy.on('proxyReq', function (proxyReq, req, res, options) {
      if (originalUrlHeader.includes('openai.com')) {
        if (process.env.OPENAI_API_KEY === undefined) {
          return reject(new Error('OPENAI_API_KEY is missing'))
        }
        req.headers['Authorization'] = `Bearer ${process.env.OPENAI_API_KEY}`
        proxyReq.setHeader('Authorization', `Bearer ${process.env.OPENAI_API_KEY}`)
      }
      if (originalUrlHeader.includes('anthropic')) {
        if (process.env.ANTHROPIC_API_KEY === undefined) {
          return reject(new Error('ANTHROPIC_API_KEY is missing'))
        }
        proxyReq.setHeader('x-api-key', process.env.ANTHROPIC_API_KEY)
      }
    })

    proxy.web(req, res, { target: originalUrlHeader, changeOrigin: true }, (err) => {
      if (err) {
        return reject(err)
      }
      resolve()
    })
  })
}

export default proxyApi
