from ..baml_client import b
from ..baml_client.globals import DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME
import pytest

DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.configure_boundary_uploader(
    project_id="project123", api_key="INVALID_API_KEY", base_url="https://abe8c5ez29.execute-api.us-east-1.amazonaws.com"
)

@pytest.mark.asyncio
async def test_studio():
    people = await b.ExtractPeople("Harry Houdini, David Copperfield")
    print(people)

    # print("Waiting for 5 seconds...")
    # DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush(5)
    # print("Done waiting.")
