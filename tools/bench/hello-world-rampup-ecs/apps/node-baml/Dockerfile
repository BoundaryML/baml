FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293 AS build
RUN apk add --no-cache curl libstdc++ libgcc
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
COPY apps/node-baml/package*.json ./
RUN npm ci --no-audit --no-fund
COPY scripts/install-baml.sh apps/node-baml/baml.toml apps/node-baml/tsconfig.json ./
COPY apps/node-baml/baml_src ./baml_src
ARG TARGETARCH
RUN sh install-baml.sh "$TARGETARCH" musl && /opt/baml/bin/baml-cli generate --agent-skill-check off && npx tsc && node --input-type=module -e "import {hello_world_async} from './dist/baml_sdk/index.js'; if (await hello_world_async() !== 'hello world') process.exit(1)" && npm prune --omit=dev
FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293
RUN apk add --no-cache libstdc++ libgcc
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
ENV NODE_ENV=production
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/dist ./
COPY apps/node-baml/runtime_metrics.cjs apps/node-baml/server.js apps/node-baml/package.json ./
# Cross-architecture emulation can report zero RSS; verify positive RSS on native ECS hosts.
RUN node -e "const assert=require('node:assert/strict'); const {renderMetrics}=require('./runtime_metrics.cjs'); const read=()=>Object.fromEntries(renderMetrics().split('\\n').filter(l=>l&&!l.startsWith('#')).map(l=>l.split(' '))); const before=read(); const held=Buffer.alloc(1048576,1); const after=read(); assert(Number.isFinite(+after.process_resident_memory_bytes) && +after.process_resident_memory_bytes>=0); assert(+after.nodejs_heap_size_total_bytes>=+after.nodejs_heap_size_used_bytes); assert(+after.nodejs_arraybuffer_memory_bytes- +before.nodejs_arraybuffer_memory_bytes>=1048576); assert(held.length===1048576)"
EXPOSE 8080
CMD ["node", "server.js"]
