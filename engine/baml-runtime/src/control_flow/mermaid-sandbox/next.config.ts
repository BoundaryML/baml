import type { NextConfig } from "next";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const headersDir = path.resolve(__dirname, "..");

const watchedExtensions = new Set([".mmd", ".baml", ".snap"]);

const collectWatchedFiles = (dir: string): string[] => {
  const files: string[] = [];
  if (!fs.existsSync(dir)) {
    return files;
  }

  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "mermaid-sandbox" || entry.name === "node_modules") {
      continue;
    }

    const fullPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      files.push(...collectWatchedFiles(fullPath));
      continue;
    }

    if (entry.isFile() && watchedExtensions.has(path.extname(entry.name))) {
      files.push(fullPath);
    }
  }

  return files;
};

class WatchHeaderFilesPlugin {
  private directory: string;

  constructor(directory: string) {
    this.directory = directory;
  }

  apply(compiler: any) {
    compiler.hooks.afterCompile.tap("WatchHeaderFilesPlugin", (compilation) => {
      compilation.contextDependencies.add(this.directory);

      for (const file of collectWatchedFiles(this.directory)) {
        compilation.fileDependencies.add(file);
      }
    });
  }
}

const nextConfig: NextConfig = {
  webpack(config, { dev }) {
    if (dev) {
      config.plugins = config.plugins ?? [];
      config.plugins.push(new WatchHeaderFilesPlugin(headersDir));
    }

    return config;
  },
};

export default nextConfig;
