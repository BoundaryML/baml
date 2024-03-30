import pprint
import requests

payload = {
  "files": {
    "main.baml": open('inputs/main.baml').read(),
    "__tests__/ExtractVerbs/red_aardvark.json":
          open('inputs/test_case.json').read(),
  },
}

response = requests.post(
    'http://localhost:8000/fiddle',
    json=payload,
)

print(f'Status Code: {response.status_code}')
print(f'Response Body: {pprint.pprint(response.json())}')
