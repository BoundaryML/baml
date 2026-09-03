import path from "node:path"
import { fileURLToPath } from "node:url"
import { createMDX } from "fumadocs-mdx/next"

const appDirectory = path.dirname(fileURLToPath(import.meta.url))

/** @type {import('next').NextConfig} */
const nextConfig = {
  devIndicators: false,
  experimental: {
    optimizePackageImports: ["lucide-react"],
  },
  turbopack: {
    root: path.resolve(appDirectory, "../../.."),
  },
}

export default createMDX()(nextConfig)
