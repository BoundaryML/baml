FROM node:18

# Create and change to the app directory.
WORKDIR /usr/src/app

# RUN npm install -g pnpm

# Copy package.json and pnpm-lock.yaml to the working directory
COPY package.json package-lock.json ./

# Install production dependencies.
RUN npm install --prod

# Copy local code to the container image.
COPY . .

# Run the web service on container startup.
CMD [ "node", "server.js" ]