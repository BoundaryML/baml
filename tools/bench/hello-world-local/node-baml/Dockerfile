FROM node:20-alpine@sha256:fb4cd12c85ee03686f6af5362a0b0d56d50c58a04632e6c0fb8363f609372293
RUN apk add --no-cache libstdc++ libgcc
WORKDIR /app
ENV NODE_ENV=production
COPY .build/node-baml/node_modules ./node_modules
COPY node-baml/package.json node-baml/server.js node-baml/runtime_metrics.cjs ./
COPY node-baml/dist/baml_sdk ./baml_sdk
EXPOSE 8080 9091
CMD ["node", "server.js"]
