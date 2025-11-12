/**
 * Debug mode BAML fixtures
 * These are loaded when debug mode is enabled
 */

export const DEBUG_BAML_FILES = {
  'baml_src/clients.baml': `// Define the GPT4o client
client<llm> GPT4o {
  provider openai
  options {
    model "gpt-4o"
    api_key env.OPENAI_API_KEY
  }
}
`,

  'baml_src/main.baml': `// This is a BAML config file, which extends the Jinja2 templating language to write LLM functions.

class Resume {
  name string
  education Education[] @description("Extract in the same order listed")
  skills string[] @description("Only include programming languages")
}

class Education {
  school string
  degree string
  year int
}

function ExtractResume(resume_text: string) -> Resume {
  // see clients.baml
  client GPT4o

  // The prompt uses Jinja syntax. Change the models or this text and watch the prompt preview change!
  prompt #"
    Parse the following resume and return a structured representation of the data in the schema below.

    Resume:
    ---
    {{ resume_text }}
    ---

    {# special macro to print the output instructions. #}
    {{ ctx.output_format }}

    JSON:
  "#
}

function CheckAvailability(day: string) -> bool {
  client GPT4o

  prompt #"
    Is the office open on {{ day }}?
    The office is open Monday through Friday.

    Return true if open, false if closed.

    {{ ctx.output_format }}
  "#
}

function CountItems(text: string) -> int {
  client GPT4o

  prompt #"
    Count how many items are mentioned in the following text:

    {{ text }}

    {{ ctx.output_format }}
  "#
}

function ParseResume(resume_text: string) -> Resume {
  client GPT4o

  prompt #"
    Parse the resume below:

    {{ resume_text }}

    {{ ctx.output_format }}
  "#
}

test Test1 {
  functions [ExtractResume]
  args {
    resume_text #"
      John Doe

      Education
      - University of California, Berkeley
        - B.S. in Computer Science
        - 2020

      Skills
      - Python
      - Java
      - C++
    "#
  }
}

test CheckAvailabilityTest {
  functions [CheckAvailability]
  args {
    day "Monday"
  }
}

test ParseResumeTest {
  functions [ParseResume]
  args {
    resume_text #"
      Jane Smith

      Education
      - MIT
        - M.S. in AI
        - 2022

      Skills
      - Python
      - TypeScript
    "#
  }
}
`,

  'baml_src/workflows/simple.baml': `function SimpleWorkflow(user_id: string) -> string {
  //# gather applicant context
  let profile = FetchData(user_id);

  //# normalize profile signals
  let normalized_profile = ProcessData(profile);

  //# persist summarized profile
  SaveResult(normalized_profile);

  normalized_profile
}

function FetchData(user_id: string) -> string {
  //# collect source material
  client GPT4o

  prompt #"
    You are the collection stage for the debugging workflow.

    Provide a compact JSON object called 'profile' that captures the most
    relevant public details you can infer for the candidate {{ user_id }}.

    {{ ctx.output_format }}
  "#
}

function ProcessData(profile: string) -> string {
  //# enrich and score content
  client GPT4o

  prompt #"
    Normalize the following profile information and extract a concise
    summary with the keys 'highlights', 'risks', and 'recommended_next_action'.

    Profile:
    {{ profile }}

    {{ ctx.output_format }}
  "#
}

function SaveResult(summary: string) -> string {
  //# confirm archiving event
  client GPT4o

  prompt #"
    A teammate just produced the summary below.

    {{ summary }}

    Respond with a short acknowledgement confirming it was persisted.
    {{ ctx.output_format }}
  "#
}

test SimpleWorkflowTest {
  functions [SimpleWorkflow]
  args {
    user_id "user_12345"
  }
}
`,

  'baml_src/workflows/conditional.baml': `class ValidationInsight {
  summary string
  flag bool
}

function ConditionalWorkflow(task_summary: string) -> string {
  //# validate payload structure
  let validation = ValidateInput(task_summary);

  //# check summary confidence
  if (CheckCondition(validation.summary)) {
    //# run enrichment subgraph
    let enriched = SubgraphProcess(task_summary);

    //# finalize success report
    return SubgraphValidate(enriched);
  } else {
    //# return remediation guidance
    return HandleFailure(task_summary);
  }
}

function ValidateInput(task_summary: string) -> ValidationInsight {
  //# sanity check request
  client GPT4o

  prompt #"
    Review the following task description and decide if it is ready for
    automation. Summarize the key intent and set 'flag' to true when the
    requirements are explicit.

    Task:
    {{ task_summary }}

    {{ ctx.output_format }}
  "#
}

function CheckCondition(summary: string) -> bool {
  //# decide routing condition
  client GPT4o

  prompt #"
    You are the routing guard for a workflow. Read the summary below and
    reply with 'true' if it is confident and actionable, otherwise 'false'.

    {{ summary }}
  "#
}

function SubgraphProcess(task_summary: string) -> string {
  //# enrich successful task
  client GPT4o

  prompt #"
    Produce a structured plan with three bullet points for completing the
    task below. Label the bullets as 'collect', 'analyze', and 'deliver'.

    {{ task_summary }}

    {{ ctx.output_format }}
  "#
}

function SubgraphValidate(enriched_plan: string) -> string {
  //# verify subgraph output
  client GPT4o

  prompt #"
    Double-check the enriched plan below and respond with a JSON object
    that includes 'status', 'risk', and a short 'next_step' summary.

    {{ enriched_plan }}

    {{ ctx.output_format }}
  "#
}

function HandleFailure(task_summary: string) -> string {
  //# emit remediation guidance
  client GPT4o

  prompt #"
    The workflow could not continue with the provided task. Explain what is
    missing and provide a single actionable next step.

    {{ task_summary }}

    {{ ctx.output_format }}
  "#
}
`,
};
