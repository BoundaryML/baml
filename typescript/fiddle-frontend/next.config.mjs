/** @type {import('next').NextConfig} */
const nextConfig = {
  transpilePackages: ['jotai-devtools', '@baml/playground-common', '@gloo-ai/baml-schema-wasm-web', '@baml/common'],
  // productionBrowserSourceMaps: true,
  eslint: {
    ignoreDuringBuilds: true,
  },
  webpack(config, { isServer, dev }) {
    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
      syncWebAssembly: true,
      layers: true,
      topLevelAwait: true,
    }

    if (dev) {
      // config.devtool = 'eval-source-map'
    }

    if (!isServer) {
      // watch my locak pnpm package @gloo-ai/playground-common for changes
      config.watchOptions = {
        ...config.watchOptions,
        // Assuming you want to ignore all in node_modules except your package
        ignored: [
          // Ignore everything in node_modules except the workspace package
          '**/node_modules/!(@baml/playground-common)**',
        ],
      }
    }
    return config
  },
  headers: async () => {
    return [
      {
        source: '/embed',
        headers: [
          {
            key: 'X-Frame-Options',
            value: 'ALLOW-ALL', // Older method, may be ignored by some browsers
          },
          // {
          //   key: 'Content-Security-Policy',
          //   value:
          //     "default-src 'self'; frame-ancestors *; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline';", // Allow embedding everywhere and remove frame restrictions
          // },
        ],
      },
    ]
  },
}

export default nextConfig
