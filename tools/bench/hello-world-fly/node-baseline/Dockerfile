FROM node:20-alpine@sha256:afdf98210b07b586eb71fa22ba2e432e058e4cd1304d31ed60888755b8c865fb
WORKDIR /app
ENV NODE_ENV=production
COPY package*.json ./
RUN npm ci --omit=dev --no-audit --no-fund
COPY runtime_metrics.cjs server.js ./
RUN node -e "const assert=require('node:assert/strict'); const {renderMetrics}=require('./runtime_metrics.cjs'); const read=()=>Object.fromEntries(renderMetrics().split('\\n').filter(l=>l&&!l.startsWith('#')).map(l=>l.split(' '))); const before=read(); const held=Buffer.alloc(1048576,1); const after=read(); assert(+after.process_resident_memory_bytes>0); assert(+after.nodejs_heap_size_total_bytes>=+after.nodejs_heap_size_used_bytes); assert(+after.nodejs_arraybuffer_memory_bytes- +before.nodejs_arraybuffer_memory_bytes>=1048576); assert(held.length===1048576)"
EXPOSE 8080
CMD ["node", "server.js"]
