import pprint
import requests

payload = {
  "files": [
    {"name": "baml_src/main.baml", "content": open('inputs/main.baml').read()},
    {"name": "baml_src/__tests__/ExtractVerbs/red_aardvark.json", "content": open('inputs/test_case.json').read()},
  ],
}

response = requests.post(
    'http://localhost:8000/fiddle',
    json=payload,
    stream=True,
)

if response.encoding is None:
    response.encoding = 'utf-8'

for line in response.iter_lines(decode_unicode=True):
    if line:  # filter out keep-alive new lines
        print(line)