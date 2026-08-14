# syntax=docker/dockerfile:1.7
#
# baml-bench UI - small Next.js app that displays the most recent benchmark
# runs and open issues from the api service (the sole Convex gateway).
# Read-only by design: there are no run controls and no writes.
#
# Built with Next.js standalone output, so the runtime image is just
# node:22-alpine + .next/standalone + .next/static.
#
# Build context is the baml-bench package root; the Dockerfile expects to
# find ui/ in the context.

FROM node:22-alpine AS deps
WORKDIR /app
RUN corepack enable
COPY ui/package.json ui/pnpm-lock.yaml* ./
RUN --mount=type=cache,target=/root/.local/share/pnpm/store,id=baml-bench-ui-pnpm \
    pnpm install --frozen-lockfile

FROM node:22-alpine AS builder
WORKDIR /app
RUN corepack enable
COPY --from=deps /app/node_modules ./node_modules
COPY ui/ ./
RUN pnpm build

FROM node:22-alpine AS runner
WORKDIR /app
ENV NODE_ENV=production
ENV PORT=9877
ENV HOSTNAME=0.0.0.0
RUN addgroup --system --gid 1500 nextjs \
    && adduser  --system --uid 1500 --ingroup nextjs nextjs

# Standalone output bundles a minimal node_modules subset + server.js.
# .next/static is not in the standalone bundle and must be copied separately.
# (No public/ assets to copy: this UI ships no static files.)
COPY --from=builder --chown=nextjs:nextjs /app/.next/standalone ./
COPY --from=builder --chown=nextjs:nextjs /app/.next/static     ./.next/static

USER nextjs
EXPOSE 9877
CMD ["node", "server.js"]
