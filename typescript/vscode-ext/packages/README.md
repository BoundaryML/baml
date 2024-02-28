# Baml Language VS Code Extension

This VS Code extension provides support for the Baml language used to define function-based LLM pipelines.

### General features

1. **Auto-build on Save**: Anytime a `.baml` file is saved, the build script is automatically triggered.
2. **Syntax Highlighting**: Provides enhanced readability and coding experience by highlighting the Baml language syntax for any file with the `.baml` extension.

## Prerequisites

### MacOS

First, install the **baml CLI**:

1. `brew tap boundaryml/baml`
2. `brew install baml`

To update the CLI

1. `brew update`
2. `brew upgrade baml`

See [the documentation](https://docs.boundaryml.com/mdx/installation) for full instructions for all platforms

## Usage

Create a baml_src directory with a main.baml file and you're all set!

**Auto-build on Save**:
When you save your `.baml` files (`Ctrl+S`), the build script (`baml build`) will automatically run.

---

For any issues, feature requests, or contributions, please reach out at contact@boundaryml.com
