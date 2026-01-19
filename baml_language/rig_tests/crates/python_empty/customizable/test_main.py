#!/usr/bin/env python3
"""Pytest tests for empty fixture."""


def test_imports():
    """Test that baml_client can be imported."""
    # This will fail if generated code has issues
    import baml_client  # noqa: F401


def test_fixture_specific():
    """Fixture-specific tests for empty."""
    # CUSTOM CHANGE: This should be preserved!
    print("✓ Custom test for empty fixture")
    assert True, "Custom assertion"
