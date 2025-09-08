function getNextJsVersion() {
  try {
    // Try to find Next.js in the project's dependencies first
    const projectNextPath = require.resolve('next/package.json', {
      paths: [process.cwd()],
    });
    const nextPackageJson = require(projectNextPath);
    return nextPackageJson.version || null;
  } catch {
    try {
      // Fallback to checking in the plugin's dependencies
      const nextPackageJson = require('next/package.json');
      return nextPackageJson.version || null;
    } catch {
      console.warn(
        'Warning: Could not determine Next.js version, defaulting to latest config',
      );
      return null;
    }
  }
}

export function withBaml(_bamlConfig = {}) {
  // eslint-disable-line no-unused-vars
  return function withBamlConfig(nextConfig = {}) {
    const nextVersion = getNextJsVersion();
    // Default to new config (>= 14) if version can't be determined
    const majorVersion = nextVersion
      ? Number.parseInt(nextVersion.split('.')[0], 10)
      : 14;
    const useNewConfig = majorVersion >= 14;
    const isTurbo = Boolean(process.env.TURBOPACK === '1');

    return {
      ...nextConfig,
      ...(useNewConfig
        ? {
            // Always add BAML to serverExternalPackages for both webpack and Turbopack
            serverExternalPackages: [
              ...(nextConfig?.serverExternalPackages || []),
              '@boundaryml/baml',
              '@boundaryml/baml-darwin-arm64',
              '@boundaryml/baml-darwin-x64',
              '@boundaryml/baml-linux-arm64-gnu',
              '@boundaryml/baml-linux-arm64-musl',
              '@boundaryml/baml-linux-x64-gnu',
              '@boundaryml/baml-linux-x64-musl',
              '@boundaryml/baml-win32-arm64-msvc',
              '@boundaryml/baml-win32-x64-msvc',
            ],
          }
        : {
            experimental: {
              ...(nextConfig.experimental || {}),
              serverComponentsExternalPackages: [
                ...(nextConfig.experimental?.serverComponentsExternalPackages ||
                  []),
                '@boundaryml/baml',
              ],
            },
          }),
      webpack: (config, context) => {
        let webpackConfig = config;
        if (typeof nextConfig.webpack === 'function') {
          webpackConfig = nextConfig.webpack(config, context);
        }

        if (context.isServer) {
          // Externalize the native module
          webpackConfig.externals = [
            ...(Array.isArray(webpackConfig.externals)
              ? webpackConfig.externals
              : []),
            '@boundaryml/baml',
            '@boundaryml/baml-darwin-arm64',
            '@boundaryml/baml-darwin-x64',
            '@boundaryml/baml-linux-arm64-gnu',
            '@boundaryml/baml-linux-arm64-musl',
            '@boundaryml/baml-linux-x64-gnu',
            '@boundaryml/baml-linux-x64-musl',
            '@boundaryml/baml-win32-arm64-msvc',
            '@boundaryml/baml-win32-x64-msvc',
          ];
        }

        // Only add webpack rules if not using Turbo
        if (!isTurbo) {
          webpackConfig.module = webpackConfig.module || {};
          webpackConfig.module.rules = webpackConfig.module.rules || [];
          webpackConfig.module.rules.push({
            test: /\.node$/,
            use: [
              {
                loader: 'nextjs-node-loader',
                options: {
                  outputPath: webpackConfig.output?.path,
                },
              },
            ],
          });
        }

        return webpackConfig;
      },
    };
  };
}
