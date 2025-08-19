import pytest
from ..baml_client import b
from ..baml_client.type_builder import TypeBuilder


class TestDynamicEnumRequest:
    """Test dynamic enum functionality using the request API."""

    @pytest.mark.asyncio
    async def test_render_dynamic_enum_with_enum_values(self):
        """Test RenderDynamicEnum request includes dynamic enum values."""
        tb = TypeBuilder()

        # Add values to existing DynEnumOne
        tb.DynEnumThree.add_value("TRIPOD").alias("for use with cameras")

        request = await b.request.RenderDynamicEnum("TRICYCLE", "TRIPOD", {"tb": tb})

        request_body = request.body.json()
        assert request_body["model"] == "gpt-4o-mini"
        assert len(request_body["messages"]) == 1
        assert request_body["messages"][0]["role"] == "system"

        # Verify the enum values are included in the schema/prompt
        message_content = request_body["messages"][0]["content"][0]["text"]

        assert (
            message_content
            == """"DynEnumThree.TRICYCLE" renders as: bike with three wheels
"other" renders as: TRIPOD

Available dynamic enum values:
  - TRICYCLE: bike with three wheels
  - TRIANGLE: TRIANGLE

Enum comparison tests:

DynEnumThree matches TRICYCLE enum value, as expected

DynEnumThree is not TRIANGLE, as expected

DynEnumThree equals TRICYCLE string, as expected

DynEnumThree is not equal to TRIANGLE string, as expected

Multiple value tests:

DynEnumThree is either TRICYCLE or TRIANGLE

Other is not TRICYCLE

Other is TRIPOD
"""
        )
