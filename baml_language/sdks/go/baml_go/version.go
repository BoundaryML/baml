package baml_go

import (
	"runtime/debug"
	"strings"
)

// DefaultRuntimeVersion is the exact BAML native runtime required by this Go
// module. The release planner stamps it alongside every other SDK version.
const DefaultRuntimeVersion = "0.15.0"

const goModulePath = "github.com/boundaryml/baml-go"

func requiredRuntimeVersion() string {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return DefaultRuntimeVersion
	}
	if info.Main.Path == goModulePath {
		if version := canonicalGoModuleVersion(info.Main.Version); version != "" {
			return version
		}
	}
	for _, dependency := range info.Deps {
		if dependency.Path != goModulePath {
			continue
		}
		version := dependency.Version
		if dependency.Replace != nil && dependency.Replace.Version != "" {
			version = dependency.Replace.Version
		}
		if version := canonicalGoModuleVersion(version); version != "" {
			return version
		}
	}
	return DefaultRuntimeVersion
}

func canonicalGoModuleVersion(version string) string {
	if version == "" || version == "(devel)" || strings.HasPrefix(version, "v0.0.0-") {
		return ""
	}
	return strings.TrimPrefix(version, "v")
}
