import pytest
from ..baml_client import b
from ..baml_client.type_builder import TypeBuilder


class TestDynamicEnumRequest:
    """Test dynamic enum functionality using the request API."""

    @pytest.mark.asyncio
    async def test_render_dynamic_enum_with_enum_values(self):
        """Test RenderDynamicEnum request includes dynamic enum values."""
        tb = TypeBuilder()

        # Add values to RenderTestEnum
        tb.RenderTestEnum.add_value("MOTORCYCLE").alias("motorized two-wheeler")

        request = await b.request.RenderDynamicEnum("BIKE", "MOTORCYCLE", {"tb": tb})

        request_body = request.body.json()
        assert request_body["model"] == "gpt-4o-mini"
        assert len(request_body["messages"]) == 1
        assert request_body["messages"][0]["role"] == "system"

        # Verify the enum values are included in the schema/prompt
        message_content = request_body["messages"][0]["content"][0]["text"]

        assert (
            message_content
            == """"RenderTestEnum.BIKE" renders as: two-wheeled bike
"other" renders as: MOTORCYCLE

Available dynamic enum values:
  - BIKE: two-wheeled bike
  - SCOOTER: kick scooter

Enum comparison tests:

'bike' is equal to RenderTestEnum.BIKE, as expected

'bike' is not equal to RenderTestEnum.SCOOTER, as expected

'bike' equals "BIKE", as expected

'bike' is not equal to "SCOOTER", as expected

Multiple value tests:

'bike' is equal to RenderTestEnum.BIKE or RenderTestEnum.SCOOTER, as expected

'other' is not equal to RenderTestEnum.BIKE, as expected

'other' is MOTORCYCLE, as expected

'other' is MOTORCYCLE, as expected
"""
        )

    @pytest.mark.asyncio
    async def test_render_dynamic_class_with_class_properties(self):
        """Test RenderDynamicClass request includes dynamic class properties."""
        tb = TypeBuilder()

        # Add values to RenderStatusEnum and properties to RenderTestClass
        tb.RenderStatusEnum.add_value("PENDING").alias("waiting for action")
        tb.RenderTestClass.add_property("priority", tb.string()).alias(
            "task priority level"
        )

        request = await b.request.RenderDynamicClass(
            {"name": "test-item", "status": "PENDING", "priority": "high"}, {"tb": tb}
        )

        request_body = request.body.json()
        assert request_body["model"] == "gpt-4o-mini"
        assert len(request_body["messages"]) == 1
        assert request_body["messages"][0]["role"] == "system"

        # Verify the class properties are included in the schema/prompt
        message_content = request_body["messages"][0]["content"][0]["text"]
        # TODO: when alias rendering is correctly implemented for dynamic enums and classes
        # we can uncomment this. For now we add a test to ensure stability of the output format.
        # assert message_content == """Input class data:
        # {
        #     "name": "test-item",
        #     "status": "waiting for action",
        #     "task priority level": "high"
        # }
        # """
        assert (
            message_content
            == """Input class data: {
    "status": PENDING,
    "name": "test-item",
    "priority": "high",
}"""
        )
