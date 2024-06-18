type EditorFile = {
  path: string
  content: string
}
// export type ParserDBFunctionTestModel = Pick<ParserDatabase['functions'][0], 'name' | 'test_cases'>;

export type BAMLSnippet = {
  id: string
  name: string
  description: string
  file?: EditorFile
  filePath?: string
  // functionsWithTests: ParserDBFunctionTestModel[];
  testRunOutput?: any
}
