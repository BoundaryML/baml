"""Tests for BAML types defined in a subdirectory (test002_directory/)."""

import pytest
from dotenv import load_dotenv

load_dotenv()

from baml_client import b, sync_b, types


class TestExtractContact:
    """Test ExtractContact — defined in test002_directory/ subdirectory."""

    def test_sync(self):
        result = sync_b.ExtractContact(
            text="Contact Alice at alice@example.com or call 555-1234."
        )
        assert isinstance(result, types.ContactInfo)
        assert len(result.name) > 0
        assert "@" in result.email

    @pytest.mark.asyncio
    async def test_async(self):
        result = await b.ExtractContact(
            text="Reach Bob at bob@corp.io, phone 555-9999."
        )
        assert isinstance(result, types.ContactInfo)
        assert len(result.name) > 0
        assert "@" in result.email


class TestContactInfoTypes:
    """Verify ContactInfo type shape."""

    def test_optional_phone(self):
        # phone is Optional[str]
        c = types.ContactInfo(name="Test", email="t@t.com", phone=None)
        assert c.phone is None

    def test_with_phone(self):
        c = types.ContactInfo(name="Test", email="t@t.com", phone="555-0000")
        assert c.phone == "555-0000"
