#!/usr/bin/env -S uv run -q
# /// script
# requires-python = ">=3.9"
# dependencies = [
#   "google-cloud-aiplatform>=1.66.0",    # includes `vertexai` generative APIs
#   "google-genai",
# ]
# ///
"""
Usage:
  export GOOGLE_APPLICATION_CREDENTIALS=/path/to/aws-wif-config.json
  export PROJECT_ID=your-gcp-project-id
  export LOCATION=us-central1   # or e.g. us-east5, europe-west4, etc.
  uv run gemini_flash.py "hello world prompt"

Notes:
  - The script uses ADC, so WIF works if GOOGLE_APPLICATION_CREDENTIALS points
    to the external_account JSON you generated with:
      gcloud iam workload-identity-pools create-cred-config ... --aws ...
"""

import os
import sys
from typing import List

from vertexai import init as vertex_init
from vertexai.generative_models import GenerativeModel, GenerationConfig

from google import genai
from google.genai.types import HttpOptions


def main(argv: List[str]) -> int:
    prompt = (
        " ".join(argv[1:]).strip()
        if len(argv) > 1
        else "Say hi and tell me a fun cloud fact."
    )
    project = os.environ.get("PROJECT_ID") or os.environ.get("GOOGLE_CLOUD_PROJECT")
    location = os.environ.get("LOCATION", "us-central1")
    model_name = os.environ.get("MODEL_NAME", "gemini-2.5-flash")

    if not project:
        print("ERROR: Set PROJECT_ID (or GOOGLE_CLOUD_PROJECT).", file=sys.stderr)
        return 2

    # Initialize Vertex AI (uses Application Default Credentials)
    vertex_init(project=project, location=location)

    model = GenerativeModel(model_name)

    # Adjust as desired
    config = GenerationConfig(
        # temperature=0.7,
        # top_p=0.95,
        # top_k=40,
        max_output_tokens=512,
    )

    print(
        f"Project: {project}\nLocation: {location}\nModel: {model_name}\n---\nPrompt:\n{prompt}\n---\nResponse:\n"
    )

    # Simple single-turn text generation
    resp = model.generate_content(prompt, generation_config=config)

    # Some SDK versions put text in different places; be defensive.
    text = (
        getattr(resp, "text", None)
        or "".join(
            part.text
            for part in getattr(resp, "candidates", [])[0].content.parts
            if hasattr(part, "text")
        )
        if getattr(resp, "candidates", None)
        else ""
    )

    print(text or str(resp))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
