/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  // Self-contained Node bundle for Docker - produces .next/standalone/server.js
  // which we copy into the runtime image. Tiny image (~150 MB) vs the ~500 MB
  // we'd get baking node_modules + .next directly.
  output: "standalone",
};

export default nextConfig;
