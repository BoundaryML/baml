# Regenerating proto clients for baml_inbound.proto / baml_outbound.proto

```sh
# Rust (prost) + Python (protoc) — both driven by bridge_ctypes/build.rs.
# The vendored copy under sdks/rust is COMMITTED: the published baml_bridge
# crate ships it so consumers need neither protoc nor bridge_ctypes.
cargo build -p bridge_ctypes
#   -> target/.../build/bridge_ctypes-*/out/baml_bridge.cffi.v1.rs
#   -> sdks/rust/bridge_rust/src/wire/baml_bridge.cffi.v1.rs (committed)
#   -> sdks/python/src/baml_bridge/cffi/v1/*_pb2.py(i)

# Node / TypeScript (protobufjs)
cd sdks/typescript/bridge_typescript && pnpm build:debug
#   -> sdks/typescript/bridge_typescript/typescript_src/proto/baml_cffi.{js,d.ts}
#
# The package's dist/ is build output and is not committed. A proto
# schema-only change needs no native build, but build:proto alone leaves the
# generated header off:
#   pnpm build:proto && pnpm build:tag-generated-files

# TypeScript (ts-proto / buf) — typescript2/pkg-proto consumer
cd typescript2/pkg-proto && pnpm generate
#   -> typescript2/pkg-proto/src/generated/baml_bridge/cffi/v1/*.ts

# Go (protoc-gen-go)
cd sdks/go/bridge_go && ./build.sh
#   -> sdks/go/bridge_go/cffi/proto/baml_bridge/cffi/v1/*.pb.go
# Keep baml_go's internal inbound/outbound consumers byte-identical:
cp cffi/proto/baml_bridge/cffi/v1/baml_inbound.pb.go \
  ../baml_go/internal/cffi/baml_inbound.pb.go
cp cffi/proto/baml_bridge/cffi/v1/baml_outbound.pb.go \
  ../baml_go/internal/cffi/baml_outbound.pb.go

# C++ (pinned vendored protoc)
cd ../../.. && cargo test -p sdkgen_cpp --test pb_generation regenerate -- --ignored --exact
#   -> sdks/cpp/bridge_cpp/pb/baml_bridge/cffi/v1/*.pb.{h,cc}

# Swift (protoc-gen-swift; macOS: brew install protobuf swift-protobuf)
sdks/swift/scripts/generate-protos.sh
#   -> sdks/swift/Sources/BamlBridge/Proto/*.pb.swift
#   -> sdks/swift/Sources/BamlBridge/Proto/.generated-from (input-hash
#      manifest; CI checks it on Linux instead of running the plugin)
```

Other consumers (`bridge_cffi`, `bridge_wasm`, `sdks/python/rust/bridge_python`) use the Rust prost types via `bridge_ctypes` — nothing extra to regenerate. `sdks/rust/bridge_rust` is the exception: it vendors the generated file (see above) because it publishes to crates.io and must not depend on this engine-coupled crate.

No clients exist for Ruby, Java/Kotlin, C#/.NET, or PHP.
