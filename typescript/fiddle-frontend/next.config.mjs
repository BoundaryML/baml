/** @type {import('next').NextConfig} */
const nextConfig = {
  transpilePackages: ['jotai-devtools', '@baml/playground-common', '@gloo-ai/baml-schema-wasm-web', '@baml/common'],
  rewrites: async () => {
    return [
      {
        source: '/anthropic/:path*',
        destination: 'https://api.anthropic.com/:path*',
      },
    ]
  },
  webpack(config, { isServer, dev }) {
    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
      syncWebAssembly: true,
      layers: true,
      topLevelAwait: true,
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
}

export default nextConfig
