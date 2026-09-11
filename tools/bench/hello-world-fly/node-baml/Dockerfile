FROM node:20-alpine@sha256:afdf98210b07b586eb71fa22ba2e432e058e4cd1304d31ed60888755b8c865fb AS build
RUN apk add --no-cache curl libstdc++ libgcc
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
COPY package*.json ./
RUN npm ci --no-audit --no-fund
COPY install-baml.sh baml.toml tsconfig.json ./
COPY baml_src ./baml_src
RUN sh install-baml.sh x86_64-unknown-linux-musl && /opt/baml/bin/baml-cli generate --agent-skill-check off && npx tsc && node --input-type=module -e "import {hello_world_async} from './dist/baml_sdk/index.js'; if (await hello_world_async() !== 'hello world') process.exit(1)" && npm prune --omit=dev
FROM node:20-alpine@sha256:afdf98210b07b586eb71fa22ba2e432e058e4cd1304d31ed60888755b8c865fb
RUN apk add --no-cache libstdc++ libgcc
WORKDIR /app
ENV BAML_PROFILE=0 BAML_TELEMETRY_DISABLED=1 BAML_LOG=off
ENV NODE_ENV=production
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/dist ./
COPY runtime_metrics.cjs server.js package.json ./
RUN node -e "const assert=require('node:assert/strict'); const {renderMetrics}=require('./runtime_metrics.cjs'); const read=()=>Object.fromEntries(renderMetrics().split('\\n').filter(l=>l&&!l.startsWith('#')).map(l=>l.split(' '))); const before=read(); const held=Buffer.alloc(1048576,1); const after=read(); assert(+after.process_resident_memory_bytes>0); assert(+after.nodejs_heap_size_total_bytes>=+after.nodejs_heap_size_used_bytes); assert(+after.nodejs_arraybuffer_memory_bytes- +before.nodejs_arraybuffer_memory_bytes>=1048576); assert(held.length===1048576)"
EXPOSE 8080
CMD ["node", "server.js"]
