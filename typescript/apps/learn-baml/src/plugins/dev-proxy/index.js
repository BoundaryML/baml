export default function devProxyPlugin() {
  return {
    name: 'dev-proxy',
    configureWebpack() {
      if (process.env.NODE_ENV !== 'development') {
        return {};
      }

      const port = process.env.BAML_API_PORT ?? '3001';
      const target = process.env.BAML_API_ORIGIN ?? `http://localhost:${port}`;

      return {
        devServer: {
          proxy: [
            {
              context: ['/api'],
              target,
              changeOrigin: true,
            },
          ],
        },
      };
    },
  };
}
