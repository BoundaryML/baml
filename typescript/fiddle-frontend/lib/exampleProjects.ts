import { TestRunOutput } from "@/app/[project_id]/_atoms/atoms";
import { EditorFile } from "@/app/actions";
import { ParserDatabase, StringSpan } from "@baml/common";

export type ParserDBFunctionTestModel = Pick<ParserDatabase['functions'][0], 'name' | 'test_cases'>;

export type BAMLProject = {
  id: string;
  name: string;
  description: string;
  files: EditorFile[];
  functionsWithTests: ParserDBFunctionTestModel[];
  testRunOutput?: TestRunOutput;
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
    id: 'extract-verbs',
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
          // the actual object type
          content: "I am running and jumping",
          name: stringSpanTest('ExtractVerbs', 'test1'),
        }]
      }
    ],
    files: [
      {
        path: 'baml_src/main.baml',
        content: `
// extract1 !!!
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
      {
        path: 'baml_src/clients.baml',
        content: `
// testing
        `
      }
    ]
  },
  {
    id: 'extract-verbs-2',
    name: 'Extract Verbs 2',
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
          // the actual object type
          content: "I am running",
          name: stringSpanTest('ExtractVerbs', 'test1'),
        }]
      }
    ],
    files: [
      {
        path: 'baml_src/main.baml',
        content: `
// extract2!! 
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
  },
  {
    id: 'extract-verbs-3',
    name: 'Extract Verbs 3',
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
          // the actual object type
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
    model gpt-3.5-turbo
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
