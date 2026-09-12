FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293
WORKDIR /app
ENV NODE_ENV=production
COPY apps/node-only/package*.json ./
RUN npm ci --omit=dev --no-audit --no-fund
COPY apps/node-only/runtime_metrics.cjs apps/node-only/server.js ./
# Cross-architecture emulation can report zero RSS; verify positive RSS on native ECS hosts.
RUN node -e "const assert=require('node:assert/strict'); const {renderMetrics}=require('./runtime_metrics.cjs'); const read=()=>Object.fromEntries(renderMetrics().split('\\n').filter(l=>l&&!l.startsWith('#')).map(l=>l.split(' '))); const before=read(); const held=Buffer.alloc(1048576,1); const after=read(); assert(Number.isFinite(+after.process_resident_memory_bytes) && +after.process_resident_memory_bytes>=0); assert(+after.nodejs_heap_size_total_bytes>=+after.nodejs_heap_size_used_bytes); assert(+after.nodejs_arraybuffer_memory_bytes- +before.nodejs_arraybuffer_memory_bytes>=1048576); assert(held.length===1048576)"
EXPOSE 8080
CMD ["node", "server.js"]
