import react from "@vitejs/plugin-react";
import type { ProxyOptions } from "vite";
import { defineConfig } from "vitest/config";

/**
 * Proxies `/<site>/*` to a site server with the prefix removed.
 *
 * The SSE stream must reach the browser unbuffered. The Vite proxy streams
 * response bodies, so the only requirements are that the upstream request
 * does not negotiate compression and that the response is marked as not
 * transformable.
 */
function siteProxy(prefix: string, target: string): ProxyOptions {
  return {
    target,
    changeOrigin: true,
    rewrite: (path) => path.slice(prefix.length) || "/",
    // An SSE connection stays open for the life of the page.
    timeout: 0,
    proxyTimeout: 0,
    configure: (proxy) => {
      proxy.on("proxyReq", (proxyReq) => {
        proxyReq.setHeader("accept-encoding", "identity");
      });
      proxy.on("proxyRes", (proxyRes, _req, res) => {
        const contentType = proxyRes.headers["content-type"] ?? "";
        if (contentType.includes("text/event-stream")) {
          proxyRes.headers["cache-control"] = "no-cache, no-transform";
          proxyRes.headers["x-accel-buffering"] = "no";
        }
        // When a site server dies, its SSE response is aborted, not ended. The
        // proxy does not pass an abort on, so the browser would keep a dead
        // stream open and never show the disconnected state.
        proxyRes.on("close", () => {
          if (!res.writableEnded) res.destroy();
        });
      });
    },
  };
}

// The three default sites of contract section 8.1. The environment variables
// exist so that the live path can be tested against servers on other ports,
// for example a second instance of the demo that `PORT_BASE` moved.
const SITE_URLS: Record<string, string> = {
  local: process.env.LOCAL_SITE_URL ?? "http://127.0.0.1:8787",
  cloud: process.env.CLOUD_SITE_URL ?? "http://127.0.0.1:8788",
  cloud2: process.env.CLOUD2_SITE_URL ?? "http://127.0.0.1:8789",
};
const WEB_PORT = Number(process.env.WEB_PORT ?? 5173);

export default defineConfig({
  plugins: [react()],
  server: {
    // An explicit IPv4 host. The default, `localhost`, binds to `::1` only on macOS.
    host: "127.0.0.1",
    port: WEB_PORT,
    strictPort: true,
    // `/cloud` is also a prefix of `/cloud2`, so every key is anchored at the path segment.
    proxy: Object.fromEntries(
      Object.entries(SITE_URLS).map(([site, target]) => [`^/${site}(/|$)`, siteProxy(`/${site}`, target)]),
    ),
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
