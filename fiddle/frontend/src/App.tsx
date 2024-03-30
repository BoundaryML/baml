import React, { useState } from 'react';
import { useMutation, QueryClient, QueryClientProvider } from '@tanstack/react-query';

const defaultMainBaml = `
generator lang_python {
  language python
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root ".."
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "pytest -s"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "poetry add baml@latest"
  package_version_command "poetry show baml"
}

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

`;

const defaultInput = `
{
  "input": "Lou and Jim Whittaker built the Rainier climbing culture Rainier from scratch. There were no outfitters anywhere near the mountain, so they took over a building and made their own store. There was nowhere to get a beer after a summit day, so they built Whittaker’s Bunkhouse bar and hotel to serve guests with more than 30 rooms. There was nowhere to throw a party after a successful trip, so Lou bought a 12-foot-by-six-foot barrel from a company that made pickles and built a hot tub that could hold 18 naked people during big celebrations."
}
`;



// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------

const queryClient = new QueryClient()

function BamlTestResults(args: {
  status: 'idle' | 'pending' | 'success' | 'error', data: any
}): JSX.Element {
    switch(args.status) {
        case 'idle':
          return <div>Submit to run tests!</div>
        case 'pending':
          return <div>Running tests...</div>
        case 'success':
          return <div>{JSON.stringify(args.data)}</div>
        case 'error':
          return <div>uh-oh omething went wrong</div>;
    }
}

function Form() {
  // State to hold the values of the textareas
  const [textareaValue1, setTextareaValue1] = useState(defaultMainBaml);
  const [textareaValue2, setTextareaValue2] = useState(defaultInput);

  const mutation = useMutation({
    mutationFn: async (newData: any) => {
      console.log("issuing request to backend")
      const response = await fetch('http://localhost:8000/fiddle', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(newData),
      });
      console.log("got response from backend")

      const res = await response.json();

      console.log(`response: ${JSON.stringify(res, null, 2)}`)
      console.log(`stdout text:\n${JSON.stringify(res.test.stdout)}`)

      return res.test.stdout;
    }
  });

  // Function to handle the form submission
  const handleSubmit = (e: any) => {
    e.preventDefault(); // Prevent the default form submit action
    console.log('Textarea 1 Value:', textareaValue1);
    console.log('Textarea 2 Value:', textareaValue2);
    // Here, you can also send the values to a server or process them further
    mutation.mutate({
      'files': {
        "main.baml": textareaValue1,
        "__tests__/ExtractVerbs/red_aardvark.json": textareaValue2,
      },
    })
  };
  return (
      <div className="App">
        <div className="page">
          <form onSubmit={handleSubmit} id="baml-form">
            <div className="col">
              <label htmlFor="textarea1">main.baml:</label>
              <textarea
                id="textarea1"
                value={textareaValue1}
                onChange={(e) => setTextareaValue1(e.target.value)}
              />
            </div>
            <div className="col">
              <label htmlFor="textarea2">__tests__/ExtractVerbs/red_aardvark.json:</label>
              <textarea
                id="textarea2"
                value={textareaValue2}
                onChange={(e) => setTextareaValue2(e.target.value)}
              />
            </div>
            <button type="submit">Submit</button>
          </form>
          

          <div>
            <BamlTestResults status={mutation.status} data={mutation.data} />
          </div>
        </div>
      </div>
  );
}

            //<BamlTestResults status='idle' data={null} /> 
function App() {

  return (
    <QueryClientProvider client={queryClient}>
      <Form/>
    </QueryClientProvider>
  );
}

export default App;
