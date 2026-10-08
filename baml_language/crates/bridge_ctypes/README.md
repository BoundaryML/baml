# Regenerating proto clients for baml_inbound.proto / baml_outbound.proto

```sh
# Rust (prost) + Python (protoc) — both driven by bridge_ctypes/build.rs.
# The vendored copy under sdks/rust is COMMITTED: the published baml_bridge
# crate ships it so consumers need neither protoc nor bridge_ctypes.
cargo build -p bridge_ctypes
#   -> target/.../build/bridge_ctypes-*/out/baml_bridge.cffi.v1.rs
#   -> sdks/rust/bridge_rust/src/wire/baml_bridge.cffi.v1.rs (committed)
#   -> sdks/python/src/baml_bridge/cffi/v1/*_pb2.py(i)

# Node / TypeScript (protobufjs + napi loader)
cd sdks/typescript/bridge_typescript && pnpm build:debug
#   -> sdks/typescript/bridge_typescript/typescript_src/proto/baml_cffi.{js,d.ts}
#   -> sdks/typescript/bridge_typescript/dist/native.js
#
# scripts/baml-language-version bump/set/sync install the pinned Node bridge
# dependencies and run this automatically after version bumps because napi
# stamps package.json's version into dist/native.js. A proto schema-only
# change needs no native build, but build:proto alone leaves the generated
# header off and the tracked dist/proto copies stale:
#   pnpm build:proto && pnpm build:copy-proto && pnpm build:tag-generated-files

# TypeScript (ts-proto / buf) — typescript2/pkg-proto consumer
cd typescript2/pkg-proto && pnpm generate
#   -> typescript2/pkg-proto/src/generated/baml_bridge/cffi/v1/*.ts

# Go (protoc-gen-go)
cd sdks/go/bridge_go && ./build.sh
#   -> sdks/go/bridge_go/cffi/proto/baml_bridge/cffi/v1/*.pb.go
# Keep baml_go's internal copies byte-identical:
cp cffi/proto/baml_bridge/cffi/v1/*.pb.go ../baml_go/internal/cffi/

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

No generated clients exist for Ruby, Java/Kotlin, C#/.NET, or PHP. The Java and C# bridges read and write the wire by hand (`sdks/java/baml_bridge` `ProtoReader`, `sdks/csharp/bridge_csharp` `BamlGeneratedCanonicalCodecs.cs`, and the C# generator's metadata encoder), as does the engine's trace encoder (`crates/bex_engine/src/trace_value_encode.rs`): a change to a message's fields is mirrored there by hand.
