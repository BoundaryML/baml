# Baml Language VS Code Extension

This VS Code extension provides support for the Baml language used to define LLM functions, test them in the integrated LLM Playground and build agentic workflows.

### General features

1. **Syntax Highlighting**: Provides enhanced readability and coding experience by highlighting the Baml language syntax for any file with the `.baml` extension.
2. **Dynamic playground**: Run and test your prompts in real-time.
3. **Build typed clients in several languages**: Command +S a baml file to build a baml client to call your functions in Python or TS.

## Usage

1. **Install BAML dependency**:

- python: `pip install baml-py`
- typescript: `npm install @boundaryml/baml`
- ruby: `bundle init && bundle add baml sorbet-runtime`

2. **Create a baml_src directory with a main.baml file and you're all set!**

   Or you can try our `init` script to get an example directory setup for you:

```bash Python
# If using your local installation, venv or conda:
pip install baml-py
baml-cli init
```

```bash TypeScript
# If using npm:
npm install @boundaryml/baml
npm run baml-cli init
```

```bash Ruby
bundle add baml
bundle exec baml-cli init
```

3. **Add your own api keys in the playground (settings icon) to test your functions**

4. See more examples at \*\*[promptfiddle.com](promptfiddle.com)

## Documentation

See our [documentation](https://docs.boundaryml.com)

For any issues, feature requests, or contributions, please reach out at contact@boundaryml.com
