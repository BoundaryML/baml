#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.9"
# dependencies = [
#   "openai>=1.93.0",
#   "azure-identity",
# ]
# ///
"""
Test Azure OpenAI with Entra ID.

Modes:
  - Service principal: set AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET
    (e.g. infisical run --env=test -- uv run test_azure_openai_entra.py)
  - Default credential chain: set USE_DEFAULT_CREDENTIAL=1 and run after az login
    (e.g. USE_DEFAULT_CREDENTIAL=1 uv run test_azure_openai_entra.py)

Required env vars (always): AZURE_OPENAI_RESOURCE_NAME, AZURE_OPENAI_DEPLOYMENT_ID
"""
import os
import sys

from azure.identity import (
    ClientSecretCredential,
    DefaultAzureCredential,
    get_bearer_token_provider,
)
from openai import AzureOpenAI

RESOURCE_VARS = ["AZURE_OPENAI_RESOURCE_NAME", "AZURE_OPENAI_DEPLOYMENT_ID"]
SP_VARS = ["AZURE_TENANT_ID", "AZURE_CLIENT_ID", "AZURE_CLIENT_SECRET"]


def main() -> None:
    use_default = os.environ.get("USE_DEFAULT_CREDENTIAL", "").strip() in ("1", "true", "yes")

    required = RESOURCE_VARS if use_default else RESOURCE_VARS + SP_VARS
    missing = [k for k in required if not os.environ.get(k)]
    if missing:
        print("Missing env vars:", ", ".join(missing), file=sys.stderr)
        if use_default:
            print("For default credential: USE_DEFAULT_CREDENTIAL=1 and az login", file=sys.stderr)
        else:
            print("Run with: infisical run --env=test -- uv run test_azure_openai_entra.py", file=sys.stderr)
        sys.exit(1)

    resource = os.environ["AZURE_OPENAI_RESOURCE_NAME"]
    deployment = os.environ["AZURE_OPENAI_DEPLOYMENT_ID"]
    endpoint = f"https://{resource}.openai.azure.com/"

    if use_default:
        credential = DefaultAzureCredential()
        print("Using DefaultAzureCredential (CLI / env / managed identity chain)", file=sys.stderr)
    else:
        credential = ClientSecretCredential(
            tenant_id=os.environ["AZURE_TENANT_ID"],
            client_id=os.environ["AZURE_CLIENT_ID"],
            client_secret=os.environ["AZURE_CLIENT_SECRET"],
        )
    token_provider = get_bearer_token_provider(
        credential,
        "https://ai.azure.com/.default",  # data-plane scope for Azure OpenAI
    )

    client = AzureOpenAI(
        azure_endpoint=endpoint,
        api_version="2024-02-01",
        azure_ad_token_provider=token_provider,
    )

    response = client.chat.completions.create(
        model=deployment,
        messages=[
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Does Azure OpenAI support customer managed keys?"},
            {
                "role": "assistant",
                "content": "Yes, customer managed keys are supported by Azure OpenAI.",
            },
            {"role": "user", "content": "Do other Azure services support this too?"},
        ],
    )

    print(response.choices[0].message.content)


if __name__ == "__main__":
    main()
