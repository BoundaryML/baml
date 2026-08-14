package baml_go

import "testing"

func TestVersionIdentities(t *testing.T) {
	if actual := GetToolchainVersion(); actual != ToolchainVersion {
		t.Errorf("GetToolchainVersion() = %q, want %q", actual, ToolchainVersion)
	}
	if actual := GetBridgeRuntimeVersion(); actual != BridgeRuntimeVersion {
		t.Errorf("GetBridgeRuntimeVersion() = %q, want %q", actual, BridgeRuntimeVersion)
	}
}
