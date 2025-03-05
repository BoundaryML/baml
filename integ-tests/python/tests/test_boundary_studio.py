from ..baml_client import b
from ..baml_client.globals import DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME
import baml_py
import pytest

# DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.configure_boundary_uploader(
#     project_id="project123", api_key="INVALID_API_KEY", base_url="https://abe8c5ez29.execute-api.us-east-1.amazonaws.com"
# )
DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.configure_boundary_uploader(
    project_id="project123", api_key="INVALID_API_KEY", base_url="https://iib08drq76.execute-api.us-east-1.amazonaws.com"
)

@pytest.mark.asyncio
async def test_studio():
    res = await b.TestImageListInput(
        imgs=[
            baml_py.Image.from_url(
                "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
            ),
            baml_py.Image.from_url(
                "https://www.google.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png"
            ),
        ]
    )

    a = "a"  # dummy
    res = await b.FnOutputBool(a)
    assert res == True

    integer = await b.FnOutputInt(a)
    assert integer == 5

    literal_integer = await b.FnOutputLiteralInt(a)
    assert literal_integer == 5

    literal_bool = await b.FnOutputLiteralBool(a)
    assert literal_bool == False

    literal_string = await b.FnOutputLiteralString(a)
    assert literal_string == "example output"

    list = await b.FnOutputClassList(a) # Broken
    assert len(list) > 0
    assert len(list[0].prop1) > 0

    classWEnum = await b.FnOutputClassWithEnum(a)
    assert classWEnum.prop2 in ["ONE", "TWO"]

    classs = await b.FnOutputClass(a)
    assert classs.prop1 is not None
    assert classs.prop2 == 540

    enumList = await b.FnEnumListOutput(a)
    assert len(enumList) == 2

    myEnum = await b.FnEnumOutput(a)
    # As no check is added for myEnum, adding a simple assert to ensure the call was made
    assert myEnum is not None

    # print("Waiting for 5 seconds...")
    # DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush(5)
    # print("Done waiting.")
