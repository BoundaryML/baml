FROM node:24-slim

# Create and change to the app directory.
WORKDIR /usr/src/app

# Install production dependencies from package.json.
COPY package.json ./
RUN npm install --prod

# Copy all app code.
COPY . .

# Drop root: run the service as the image's built-in unprivileged `node` user.
USER node

# Run the web service on container startup.
CMD [ "node", "server.js" ]
