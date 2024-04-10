import pprint
import requests

payload = {
  "files": [
    {"name": "baml_src/main.baml", "content": open('inputs/main.baml').read()},
    {"name": "baml_src/__tests__/ExtractVerbs/red_aardvark.json", "content": open('inputs/test_case.json').read()},
  ],
}

prod_url = "https://prompt-fiddle.fly.dev"
local_url = "http://localhost:8000"

try:
    response = requests.post(
        f'{prod_url}/fiddle',
        json=payload,  # Ensure 'payload' is defined before this line
        stream=True,
    )
    response.raise_for_status()  # Raises an HTTPError if the response status code is 4XX or 5XX

    if response.encoding is None:
        response.encoding = 'utf-8'

    for line in response.iter_lines(decode_unicode=True):
        if line:  # Filter out keep-alive new lines
            print(line)
            #pass

except requests.exceptions.HTTPError as e:
    print(f"HTTP Error: {e}")
except requests.exceptions.ConnectionError as e:
    print(f"Error Connecting: {e}")
except requests.exceptions.Timeout as e:
    print(f"Timeout Error: {e}")
except requests.exceptions.RequestException as e:
    print(f"Unexpected Error: {e}")