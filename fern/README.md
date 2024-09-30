# Fern Configuration

View the documentation [here](https://boundary.docs.buildwithfern.com).

## Updating your Docs

### Local Development server

To run a local development server with hot-reloading you can run the following command

```sh
fern docs dev
```

### Hosted URL 

Documentation is automatically updated when you push to main via the `fern generate` command. 

```sh
npm install -g fern-api # only required once
fern generate --docs
```
