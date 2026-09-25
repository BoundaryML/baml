/** @type {import('next').NextConfig} */
const nextConfig = {
  // A self-contained server bundle for the Fly image (deploy/Dockerfile).
  output: 'standalone',
};

export default nextConfig;
