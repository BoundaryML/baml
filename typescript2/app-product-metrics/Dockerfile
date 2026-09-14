# syntax=docker/dockerfile:1
FROM node:24-slim AS build
WORKDIR /workspace
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY app-product-metrics/package.json app-product-metrics/package.json
RUN pnpm install --frozen-lockfile --filter app-product-metrics
COPY app-product-metrics app-product-metrics
RUN pnpm --filter app-product-metrics build && pnpm --filter app-product-metrics deploy --legacy --prod /app

FROM node:24-slim
ENV NODE_ENV=production
WORKDIR /app
COPY --from=build --chown=node:node /app .
USER node
EXPOSE 3000
CMD ["node", "dist/server.js"]
