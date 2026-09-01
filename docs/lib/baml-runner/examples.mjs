export const helloWorld = {
  id: 'hello-world',
  functionName: 'main',
  expected: '"hello from BAML"',
  files: {
    'baml.toml': '[package]\nname = "docs-hello-world"\n',
    'baml_src/main.baml': `function main() -> string {
  "hello from BAML"
}`,
  },
};

export const runnableExamples = [helloWorld];
