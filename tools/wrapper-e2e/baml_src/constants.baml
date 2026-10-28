// Fixture identities and files shared by the OS fixture and all conformance cases.
const BASE_VERSION = "0.11.0";
const NEXT_VERSION = "0.12.0";
const LATEST_VERSION = "0.13.0";
const PROJECT_TOOLCHAIN_IDENTITY = "L1";
const CWD_TOOLCHAIN_IDENTITY = "L2";
const REINSTALLED_TOOLCHAIN_IDENTITY = "A2";
const REPLACEMENT_WRAPPER_IDENTITY = "replacement";
const USER_CONFIG_FILE = "baml/config.toml";
const CHANNEL_STATE_FILE = "baml/state.toml";
const PROJECT_MANIFEST_FILE = "project/baml.toml";

const TELEMETRY_DELIVERY_TEST_LIMIT = baml.time.Duration.from_seconds(5);

const STRESS_PAYLOAD_BYTES = 256 * 1024;
const TELEMETRY_SETTLE_TIMEOUT = baml.time.Duration.from_seconds(15);
const TELEMETRY_POLL_INTERVAL = baml.time.Duration.from_milliseconds(10);
const REPLACEMENT_TEST_TIMEOUT = baml.time.Duration.from_seconds(15);
const REPLACEMENT_POLL_INTERVAL = baml.time.Duration.from_milliseconds(50);
