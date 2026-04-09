import { existsSync, readFileSync, readdirSync, statSync } from 'fs';
import { join, resolve } from 'path';
import { fileURLToPath } from 'url';

interface EmbeddingsChunk {
  id: string;
  title: string;
  url: string;
  content: string;
  section?: string;
  embedding: number[];
}

export interface EmbeddingsIndex {
  model: string;
  dimensions: number;
  chunks: EmbeddingsChunk[];
  generatedAt?: string;
}

const appRoot = fileURLToPath(new URL('..', import.meta.url));
const docsDir = join(appRoot, 'docs');
const staticEmbeddingsPath = join(appRoot, 'static', 'embeddings.json');
const buildEmbeddingsPath = join(appRoot, 'build', 'embeddings.json');

const STALE_CHECK_INTERVAL_MS = 30_000;

let cachedIndex: EmbeddingsIndex | null = null;
let cachedIndexMtimeMs: number | null = null;
let lastFreshnessCheckMs = 0;
let lastFreshnessResult: { ok: boolean; message?: string } | null = null;

function resolveEmbeddingsPath(): string {
  if (process.env.EMBEDDINGS_PATH) {
    return resolve(appRoot, process.env.EMBEDDINGS_PATH);
  }

  if (existsSync(staticEmbeddingsPath)) return staticEmbeddingsPath;
  if (existsSync(buildEmbeddingsPath)) return buildEmbeddingsPath;

  return staticEmbeddingsPath;
}

function findLatestMdxMtime(dir: string): number | null {
  if (!existsSync(dir)) return null;

  let latest = 0;
  for (const entry of readdirSync(dir)) {
    const fullPath = join(dir, entry);
    const stat = statSync(fullPath);
    if (stat.isDirectory()) {
      const childLatest = findLatestMdxMtime(fullPath);
      if (childLatest && childLatest > latest) latest = childLatest;
    } else if (entry.endsWith('.mdx')) {
      if (stat.mtimeMs > latest) latest = stat.mtimeMs;
    }
  }

  return latest || null;
}

export function ensureEmbeddingsFresh(): { ok: boolean; message?: string } {
  if (process.env.EMBEDDINGS_SKIP_STALE_CHECK === '1') {
    return { ok: true };
  }

  const now = Date.now();
  if (lastFreshnessResult && now - lastFreshnessCheckMs < STALE_CHECK_INTERVAL_MS) {
    return lastFreshnessResult;
  }

  lastFreshnessCheckMs = now;

  const embeddingsPath = resolveEmbeddingsPath();
  if (!existsSync(embeddingsPath)) {
    lastFreshnessResult = {
      ok: false,
      message: `Embeddings not found at ${embeddingsPath}. Run "pnpm build:embeddings".`,
    };
    return lastFreshnessResult;
  }

  const embeddingsStat = statSync(embeddingsPath);
  const latestDocMtime = findLatestMdxMtime(docsDir);

  if (latestDocMtime && embeddingsStat.mtimeMs < latestDocMtime) {
    lastFreshnessResult = {
      ok: false,
      message: 'Embeddings are stale. Run "pnpm build:embeddings" and restart the server.',
    };
    return lastFreshnessResult;
  }

  lastFreshnessResult = { ok: true };
  return lastFreshnessResult;
}

export function loadEmbeddingsIndex(): EmbeddingsIndex {
  const embeddingsPath = resolveEmbeddingsPath();
  if (!existsSync(embeddingsPath)) {
    throw new Error(`Embeddings not found at ${embeddingsPath}`);
  }

  const stat = statSync(embeddingsPath);
  if (!cachedIndex || cachedIndexMtimeMs !== stat.mtimeMs) {
    const raw = readFileSync(embeddingsPath, 'utf-8');
    cachedIndex = JSON.parse(raw) as EmbeddingsIndex;
    cachedIndexMtimeMs = stat.mtimeMs;
  }

  return cachedIndex;
}
