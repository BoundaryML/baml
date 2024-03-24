# Use the official Node.js 18 image as a parent image
FROM node:18

# Set the working directory inside the container
WORKDIR /myapp

RUN apt-get update && \
    apt-get install -y jq

# Testing binaries
ENV CAPTURE_DIR="/usr/src/logs"
ENV BAML_TEST_LOGS="/usr/src/logs/baml_test"
RUN mkdir -p /usr/src/logs
COPY script.sh /usr/src/test.sh
RUN chmod +x /usr/src/test.sh
COPY --from=baml_builder /baml_source_code/target/release/baml /usr/tools/baml
ENV PATH="/usr/tools:${PATH}"

RUN npm install -g pnpm

# Initialize a new Node.js project
RUN pnpm init

# Install TypeScript and other dependencies
RUN pnpm add typescript -D
RUN pnpm add @types/node -D

RUN pnpm install

# Copy your source files into the Docker image
COPY . .

# Initialize a TypeScript project (creates tsconfig.json)
RUN npx tsc --init

# RUN npx tsc

CMD ["/usr/src/test.sh"]