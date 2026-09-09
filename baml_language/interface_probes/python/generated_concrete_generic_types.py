"""Static consumer of concrete generic method and record result types."""

from typing_extensions import assert_type

from baml_sdk import FacadeBox, FacadeRecord, facade_box_async, facade_record_async


async def check_concrete_generic_types(value: int) -> None:
    box = await facade_box_async()
    assert_type(box, FacadeBox[str])
    assert_type(await box.read(), str)
    assert_type(await box.replace("Grace"), str)
    assert_type(await box.echo(value, _types={"U": int}), int)
    assert_type(await box.same(), FacadeBox[str])
    record = await facade_record_async(box)
    assert_type(record, FacadeRecord[FacadeBox[str]])
    assert_type(record.value, FacadeBox[str])
    assert_type(await record.value.read(), str)
