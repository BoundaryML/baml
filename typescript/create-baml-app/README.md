# Create BAML App 🐑

<p align="center">
<img src="https://github.com/ashayas/create-baml-app/raw/master/.github/assets/lamb.png" width="250" alt="Create BAML App"/>
</p>

<p align="center">
<a href="https://www.npmjs.com/package/create-baml-app"><img src="https://img.shields.io/npm/v/create-baml-app.svg" alt="npm version"></a>
<a href="https://www.npmjs.com/package/create-baml-app"><img src="https://img.shields.io/npm/dm/create-baml-app.svg" alt="npm downloads"></a>
<img src="https://img.shields.io/badge/license-ISC-blue" alt="ISC License">
</p>

A CLI tool to set up a BAML project with your preferred language and package manager.


## Installation & Usage 🚀

Create a new BAML project using npx:

```bash
# Create a new project with interactive prompts
npx create-baml-app

# Create a new project in a specific directory
npx create-baml-app my-baml-app

# Create a new project in the current directory
npx create-baml-app my-baml-app --use-current-dir
# or
npx create-baml-app my-baml-app -.

# Show help menu
npx create-baml-app --help
```

When using the interactive prompts:
- Enter your project name (defaults to 'my-app')
- Use "." to create the project in the current directory
- Or enter a name to create a new directory

The CLI will then:
1. Create a new directory for your project (unless "." or --use-current-dir is specified)
2. Help you select a programming language (Python, TypeScript, Ruby)
3. Choose your preferred package manager
4. Set up a complete BAML project structure

## Supported Languages and Package Managers 🛠️

### Python
- **pip**: Standard Python package manager
  - Requires: Python installed on your system
- **poetry**: Modern Python dependency management
  - Requires: [Poetry](https://python-poetry.org/docs/#installation) installed
- **uv**: Fast Python package installer
  - Requires: [uv](https://github.com/astral-sh/uv) installed

### TypeScript
- **npm**: Standard Node.js package manager
  - Requires: [Node.js](https://nodejs.org/) installed
- **pnpm**: Fast, disk space efficient package manager
  - Requires: [pnpm](https://pnpm.io/installation) installed
- **yarn**: Alternative Node.js package manager
  - Requires: [Yarn](https://yarnpkg.com/getting-started/install) installed
- **deno**: Secure runtime for JavaScript and TypeScript
  - Requires: [Deno](https://deno.land/manual/getting_started/installation) installed
  - Note: For Deno in VSCode, add `{ "deno.unstable": ["sloppy-imports"] }` to your settings
- **bun**: Fast, disk space efficient package manager
  - Requires: [bun](https://bun.sh/docs/installation) installed

### Ruby
- **bundle**: Ruby's package manager
  - Requires: [Ruby](https://www.ruby-lang.org/en/documentation/installation/) and [Bundler](https://bundler.io/) installed

## Project Structure 📁

After running `create-baml-app`, you'll have a basic BAML project with:

- The BAML CLI installed
- Basic project structure with configuration files
- Initial BAML source files

## Development Workflow 🧑‍💻

1. Write BAML function definitions in `.baml` files
2. Generate client code with `baml-cli generate`
3. Import and use the generated code in your application

## Resources 📚

- [BAML Documentation](https://docs.boundaryml.com/)
- [BAML GitHub Repository](https://github.com/BoundaryML/baml)
- [Boundary ML Website](https://www.boundaryml.com/)

## Contributing 🤝

Contributions are welcome! Please feel free to submit a Pull Request.

## License 📄

This project is licensed under the ISC License. See the [LICENSE](LICENSE) file for details.
