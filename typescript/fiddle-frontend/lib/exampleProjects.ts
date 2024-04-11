import { EditorFile } from "@/app/actions";
import { ParserDatabase, StringSpan } from "@baml/common";

export type ParserDBFunctionTestModel = Pick<ParserDatabase['functions'][0], 'name' | 'test_cases'>;

export type BAMLProject = {
  name: string;
  description: string;
  files: EditorFile[];
  functionsWithTests: ParserDBFunctionTestModel[];
};

function stringSpanTest(functionName: string, testName: string): StringSpan {
  return {
    value: testName,
    start: 0,
    end: 0,
    source_file: `baml_src/__tests__/${functionName}/${testName}.json`,
  }
}

export const exampleProjects: BAMLProject[] = [
  {
    name: 'Extract Verbs',
    description: 'Extract verbs from a given input',
    functionsWithTests: [
      {
        name: {
          value: 'ExtractVerbs',
          start: 0,
          end: 0,
          source_file: 'baml_src/main.baml',
        },
        test_cases: [{
          content: "I am running",
          name: stringSpanTest('ExtractVerbs', 'test1'),
        }]
      }
    ],
    files: [
      {
        path: 'baml_src/main.baml',
        content: `

function ExtractVerbs {
    input string
    /// list of verbs
    output string[]
}

client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4 
    api_key env.OPENAI_API_KEY
  }
}

impl<llm, ExtractVerbs> version1 {
  client GPT4
  prompt #"
    Extract the verbs from this INPUT:
  
    INPUT:
    ---
    {#input}
    ---
    {// this is a comment inside a prompt! //}
    Return a {#print_type(output)}.

    Response:
  "#
}
`
      },
    ]
  }
];
