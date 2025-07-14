# BAML Examples for Embedding

This directory contains BAML examples that can be loaded in the embedded BAML playground.

## Directory Structure

Each example should follow this structure:
```
_docs/
  example-name/
    baml_src/
      main.baml
```

## How to Use

To embed a BAML example in an iframe, use the following URL format:

```html
<iframe src="https://your-domain.com/embed?example=example-name" width="100%" height="600px"></iframe>
```

Where `example-name` is the name of the directory containing the BAML example.

## Server-Side Loading

The BAML file is loaded on the server side for better performance. The server:
1. Reads the example name from the URL parameters
2. Sanitizes the example name to prevent directory traversal attacks
3. Loads the BAML file from the file system
4. Falls back to the default example if the specified example doesn't exist
5. Passes the BAML content to the client component

## Default Example

If no example parameter is provided, the playground will load from `_docs/default-example/baml_src/main.baml`.

## Adding New Examples

1. Create a new directory under `_docs` with your example name (use only alphanumeric characters, hyphens, and underscores)
2. Create a `baml_src` directory inside your example directory
3. Add your `main.baml` file inside the `baml_src` directory 