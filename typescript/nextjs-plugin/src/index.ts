import type { Configuration } from 'webpack'

function getNextJsVersion(): string | null {
  try {
    // Try to find Next.js in the project's dependencies first
    const projectNextPath = require.resolve('next/package.json', {
      paths: [process.cwd()],
    })
    const nextPackageJson = require(projectNextPath)
    return nextPackageJson.version || null
  } catch (error) {
    try {
      // Fallback to checking in the plugin's dependencies
      const nextPackageJson = require('next/package.json')
      return nextPackageJson.version || null
    } catch (error) {
      console.warn('Warning: Could not determine Next.js version, defaulting to latest config')
      return null
    }
  }
}

type GenericNextConfig = {
  experimental?: {
    serverComponentsExternalPackages?: string[]
    turbo?: {
      rules?: Record<string, any>
      resolveAlias?: Record<string, any>
      resolve?: {
        alias?: Record<string, any>
        conditionNames?: string[]
        preferRelative?: boolean
      }
    }
  }
  serverExternalPackages?: string[]
  webpack?: ((config: Configuration, context: any) => Configuration) | null
}

export interface BamlNextConfig {
  webpack?: ((config: Configuration, context: any) => Configuration) | null
}

export function withBaml(bamlConfig: BamlNextConfig = {}) {
  return function withBamlConfig<T extends GenericNextConfig>(nextConfig: T = {} as T): T {
    const nextVersion = getNextJsVersion()
    // Default to new config (>= 14) if version can't be determined
    const majorVersion = nextVersion ? Number.parseInt(nextVersion.split('.')[0], 10) : 14
    const useNewConfig = majorVersion >= 14
    const isTurbo = Boolean(process.env.TURBOPACK === '1')

    if (isTurbo) {
      console.warn(
        '\x1b[33m%s\x1b[0m',
        `
⚠️  Warning: @boundaryml/baml-nextjs-plugin does not yet support Turbopack
   Please remove the --turbo flag from your "next dev" command.
   Example: "next dev" instead of "next dev --turbo"
      `,
      )
    }

    const turboConfig = isTurbo
      ? {
          ...(nextConfig.experimental?.turbo || {}),
          rules: {
            ...(nextConfig.experimental?.turbo?.rules || {}),
            '*.node': { loaders: ['nextjs-node-loader'], as: '*.js' },
          },
        }
      : undefined

    return {
      ...nextConfig,
      ...(useNewConfig
        ? {
            experimental: {
              ...(nextConfig.experimental || {}),
              ...(isTurbo ? { turbo: turboConfig } : {}),
            },
            // In Turbo mode, we don't add it to serverExternalPackages to avoid the conflict
            ...(isTurbo
              ? {}
              : {
                  serverExternalPackages: [...(nextConfig?.serverExternalPackages || []), '@boundaryml/baml'],
                }),
          }
        : {
            experimental: {
              ...(nextConfig.experimental || {}),
              serverComponentsExternalPackages: [
                ...((nextConfig.experimental as any)?.serverComponentsExternalPackages || []),
                '@boundaryml/baml',
              ],
              ...(isTurbo ? { turbo: turboConfig } : {}),
            },
          }),
      webpack: (config: Configuration, context: any) => {
        if (typeof nextConfig.webpack === 'function') {
          config = nextConfig.webpack(config, context)
        }

        if (context.isServer) {
          // Externalize the native module
          config.externals = [
            ...(Array.isArray(config.externals) ? config.externals : []),
            '@boundaryml/baml',
            '@boundaryml/baml-darwin-arm64',
            '@boundaryml/baml-darwin-x64',
            '@boundaryml/baml-linux-arm64-gnu',
            '@boundaryml/baml-linux-arm64-musl',
            '@boundaryml/baml-linux-x64-gnu',
            '@boundaryml/baml-linux-x64-musl',
            '@boundaryml/baml-win32-arm64-msvc',
            '@boundaryml/baml-win32-x64-msvc',
          ]
        }

        // Only add webpack rules if not using Turbo
        if (!isTurbo) {
          config.module = config.module || {}
          config.module.rules = config.module.rules || []
          config.module.rules.push({
            test: /\.node$/,
            use: [
              {
                loader: 'nextjs-node-loader',
                options: {
                  outputPath: config.output?.path,
                },
              },
            ],
          })
        }

        // Ensure module and rules are defined
        config.module = config.module || {}
        config.module.rules = config.module.rules || []
        // Add WebAssembly loading configuration (properly indented)

        return config
      },
    } as T
  }
}
