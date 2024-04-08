baml fiddle

docker build -t fiddle .
docker run -p 8000:8000 fiddle

## Run without docker

First activate the virutal environment in fiddle/ dir
(create virtualenv if not available: `virtualenv .`)
`source bin/activate`

### Install deps

`pip install baml pytest python-dotenv`

`cd backend`
`uvicorn app.main:app --host 0.0.0.0 --port 8000 --reload`

## Test command

```
curl -X POST "http://localhost:8000/fiddle" \
-H "Content-Type: application/json" \
-d '{
  "files": [
    {
      "name": "baml_src/main.baml",
      "content": "generator lang_python {\n language python\n // This is where your non-baml source code located\n // (relative directory where pyproject.toml, package.json, etc. lives)\n project_root \"..\" \n // This command is used by \"baml test\" to run tests\n // defined in the playground\n test_command \"pytest -s\"\n // This command is used by \"baml update-client\" to install\n // dependencies to your language environment\n install_command \"poetry add baml@latest\"\n package_version_command \"poetry show baml\"\n}\n\nfunction ExtractVerbs {\n input string\n /// list of verbs\n output string[]\n}\n\nclient<llm> GPT4 {\n provider baml-openai-chat\n options {\n model gpt-4 \n api_key env.OPENAI_API_KEY\n }\n}\n\nimpl<llm, ExtractVerbs> version1 {\n client GPT4\n prompt #\"\n Extract the verbs from this INPUT:\n \n INPUT:\n ---\n {#input}\n ---\n {// this is a comment inside a prompt! //}\n Return a {#print_type(output)}.\n\n Response:\n \"#\n}"
    },
    {
      "name": "baml_src/__tests__/ExtractVerbs/test1.json",
      "content": "
    }
  ]
}'
```
