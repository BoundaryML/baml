# Gloo Language VS Code Extension

This VS Code extension provides support for the Gloo language used to define function-based LLM pipelines.

### General features

1. **Auto-build on Save**: Anytime a `.gloo` file is saved, the build script is automatically triggered.
2. **Syntax Highlighting**: Provides enhanced readability and coding experience by highlighting the Gloo language syntax for any file with the `.gloo` extension.

## Prerequisites

Gloo tooling is currently only available on macOS.

First, install the **gloo CLI**:

1. `brew tap gloohq/gloo`
2. `brew install gloo`

To update the CLI

1. `brew update`
2. `brew upgrade gloo`

## Usage

Initialize gloo in your Python project at the project root (and ensure you are using Poetry):

`gloo init`

**Auto-build on Save**:
When you save your `.gloo` files (`Ctrl+S`), the build script (`gloo build`) will automatically run.

---

For any issues, feature requests, or contributions, please reach out at contact@trygloo.com
