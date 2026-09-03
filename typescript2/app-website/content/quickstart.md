# BAML Quickstart

This guide installs the BAML toolchain, initializes a project, configures agent guidance, and runs the default entry point.

Web page: https://boundaryml.com/quickstart

## 1. Install the BAML toolchain

Choose one method for your operating system.

### Homebrew on macOS or Linux

```sh
brew install baml
```

### Install script on macOS or Linux

```sh
curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s
```

### Arch Linux

```sh
yay -S baml-bin
```

### Windows PowerShell

```powershell
irm https://pkg.boundaryml.com/install.ps1 | iex
```

Confirm that the executable is available:

```sh
baml --version
```

## 2. Initialize a project

From the repository where you want to use BAML, run:

```sh
baml init
```

This creates the initial BAML project structure. BAML namespaces follow the filesystem, so an agent can inspect the project layout directly with tools such as `ls`.

## 3. Install version-matched agent guidance

Run:

```sh
baml agent install
```

This installs the BAML skill for supported coding agents, including Claude Code and Codex. The installed skill belongs with the project and matches its BAML toolchain.

When an agent works on `.baml` files, it should use:

- The installed BAML skill for language and workflow guidance
- `baml describe` to inspect definitions, signatures, dependencies, and references
- `baml run` to execute functions
- `baml test` to run tests

Do not rely on unrelated or older online documentation when it disagrees with the installed toolchain.

## 4. Run the project

Run the default `main` function:

```sh
baml run main
```

Any BAML function can be run as a CLI target. Its parameters become command-line flags:

```sh
baml run greet -- --name "world"
baml run greet -- --help
```

For small experiments, execute a BAML expression without creating a file:

```sh
baml run -e '"a,b,c".split(",")'
```

## 5. Configure an editor

Editor support is optional. Agents can work through the CLI, and humans can also use the browser playground.

### VS Code

```sh
baml ide install --code
```

### Cursor

```sh
baml ide install --cursor
```

### Other VS Code-compatible editors

Save the extension as a `.vsix` file in the current directory:

```sh
baml ide install --path .
```

Install the generated VSIX through the editor's Extensions interface.

### Zed, JetBrains, Neovim, and other LSP clients

Run BAML as a language server:

```sh
baml lsp
```

Configure the editor to launch that command as its language server for `.baml` files.

### Browser playground

If you do not want editor integration, launch the playground:

```sh
baml playground
```

## Keeping BAML updated

The top-level `baml` wrapper is a version manager, so it rarely needs updating.

With Homebrew:

```sh
brew upgrade baml
```

Or use the built-in updater:

```sh
baml self-update
```

After changing project toolchain versions, run `baml agent install` again if the CLI tells you the installed agent guidance needs refreshing.

## Getting help

- [Explore the language](https://boundaryml.com/explore.md)
- [See how `baml describe` works](https://boundaryml.com/explore.md#baml-describe-ast-aware-discovery)
- [Open the demo repository](https://github.com/boundaryml/baml-demos)
- [Join the BAML Discord](https://boundaryml.com/discord)
- [Book a free 45-minute onboarding session](https://boundaryml.com/eap)

When asking for help, include the output of `baml --version` and the command or file that produced the problem.
