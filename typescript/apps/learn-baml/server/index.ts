import 'dotenv/config';
import { existsSync, statSync } from 'fs';
import { extname, resolve, sep } from 'path';
import { fileURLToPath } from 'url';
import { handleChatRequest } from './chat-handler';
import { handleTourReplRequest } from './tour-repl-handler';

declare const Bun: any;

const appRoot = fileURLToPath(new URL('..', import.meta.url));
const buildDir = resolve(appRoot, 'build');

const port = Number(process.env.BAML_API_PORT ?? process.env.PORT ?? 3001);
const shouldServeStatic = process.env.SERVE_STATIC === '1' || process.env.NODE_ENV === 'production';

function resolveBuildPath(requestPath: string): string | null {
  const normalized = decodeURIComponent(requestPath).replace(/^\/+/, '');
  const resolvedPath = resolve(buildDir, normalized);
  if (resolvedPath !== buildDir && !resolvedPath.startsWith(buildDir + sep)) {
    return null;
  }
  return resolvedPath;
}

function serveFile(filePath: string, status = 200): Response {
  return new Response(Bun.file(filePath), { status });
}

async function serveStatic(requestPath: string, method: string): Promise<Response> {
  if (!existsSync(buildDir)) {
    return new Response('Build output not found. Run "pnpm build" first.', { status: 500 });
  }

  if (method !== 'GET' && method !== 'HEAD') {
    return new Response('Method not allowed', { status: 405 });
  }

  let path = requestPath;
  if (path === '/' || path === '') {
    path = '/index.html';
  }

  const directPath = resolveBuildPath(path);
  if (directPath && existsSync(directPath) && statSync(directPath).isFile()) {
    return serveFile(directPath);
  }

  if (!extname(path)) {
    const indexPath = path.endsWith('/') ? `${path}index.html` : `${path}/index.html`;
    const resolvedIndexPath = resolveBuildPath(indexPath);
    if (resolvedIndexPath && existsSync(resolvedIndexPath) && statSync(resolvedIndexPath).isFile()) {
      return serveFile(resolvedIndexPath);
    }
  }

  const notFoundPath = resolveBuildPath('/404.html');
  if (notFoundPath && existsSync(notFoundPath) && statSync(notFoundPath).isFile()) {
    return serveFile(notFoundPath, 404);
  }

  return new Response('Not Found', { status: 404 });
}

const server = Bun.serve({
  port,
  async fetch(req: Request) {
    const url = new URL(req.url);

    if (url.pathname === '/api/chat') {
      return handleChatRequest(req);
    }

    if (url.pathname === '/api/tour/repl') {
      return handleTourReplRequest(req);
    }

    if (shouldServeStatic) {
      return serveStatic(url.pathname, req.method);
    }

    return new Response('Not Found', { status: 404 });
  },
});

console.log(`Learn BAML server listening on http://localhost:${server.port}`);
