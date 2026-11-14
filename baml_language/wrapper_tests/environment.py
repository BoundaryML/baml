"""Keep wrapper test processes independent of ambient cloud telemetry settings."""


def test_environment(inherited):
    return {
        **{
            key: value
            for key, value in inherited.items()
            if not key.startswith("BOUNDARY_")
        },
        "BAML_TELEMETRY": "off",
        "BAML_WRAPPER_BCS_TELEMETRY": "off",
        "BOUNDARY_API_KEY": "local",
        "DO_NOT_TRACK": "1",
    }
