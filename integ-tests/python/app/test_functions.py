import pytest

from baml_client import b
from baml_client.types import NamedArgsSingleEnumList, NamedArgsSingleClass


@pytest.mark.asyncio
async def test_should_work_for_all_inputs():
    res = await b.TestFnNamedArgsSingleBool(True)
    assert res == "true"

    res = await b.TestFnNamedArgsSingleStringList(["a", "b", "c"])
    assert "a" in res and "b" in res and "c" in res

    print("calling with class")
    res = await b.TestFnNamedArgsSingleClass(
        myArg=NamedArgsSingleClass(
            key="key",
            key_two=True,
            key_three=52,
        )
    )
    print("got response", res)
    assert "52" in res

    res = await b.TestMulticlassNamedArgs(
        myArg=NamedArgsSingleClass(
            key="key",
            key_two=True,
            key_three=52,
        ),
        myArg2=NamedArgsSingleClass(
            key="key",
            key_two=True,
            key_three=52,
        ),
    )
    assert "52" in res and "64" in res

    res = await b.TestFnNamedArgsSingleEnumList([NamedArgsSingleEnumList.TWO])
    assert "TWO" in res

    res = await b.TestFnNamedArgsSingleFloat(3.12)
    assert "3.12" in res

    res = await b.TestFnNamedArgsSingleInt(3566)
    assert "3566" in res


@pytest.mark.asyncio
async def test_should_work_for_all_outputs():
    a = "a"  # dummy
    res = await b.FnOutputBool(a)
    assert res == True

    list = await b.FnOutputClassList(a)
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


@pytest.mark.asyncio
async def test_should_work_with_image():
    pass  # TODO: Handle image testing when type definitions and support are available
