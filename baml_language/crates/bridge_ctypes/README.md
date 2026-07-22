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
# stamps package.json's version into dist/native.js. build:proto alone is only
# sufficient for proto schema-only changes.

# TypeScript (ts-proto / buf) — typescript2/pkg-proto consumer
cd typescript2/pkg-proto && pnpm generate
#   -> typescript2/pkg-proto/src/generated/baml_bridge/cffi/v1/*.ts

# Go (protoc-gen-go)
cd sdks/go/bridge_go && ./build.sh
#   -> sdks/go/bridge_go/cffi/proto/baml_bridge/cffi/v1/*.pb.go

# C++ (pinned vendored protoc; writes the committed protobuf-lite sources)
cargo test -p sdkgen_cpp --test pb_generation regenerate -- --ignored
#   -> sdks/cpp/bridge_cpp/pb/baml_bridge/cffi/v1/*.{pb.h,pb.cc}
```

Other Rust consumers (`bridge_cffi`, `bridge_wasm`, `sdks/python/rust/bridge_python`, and `sdks/java/bridge_java`) use the prost types through `bridge_ctypes`; only the vendored Rust bridge file above needs an extra committed copy. The Java runtime jar implements its wire reader/writer directly from the schema's field numbers, so schema changes must update and test `sdks/java/baml_bridge/src/main/java/baml_bridge/internal/{ProtoReader,ProtoWriter}.java` even though there is no Java protoc-generation step.

There are currently no wire clients for Ruby, C#/.NET, Swift, or PHP. Kotlin helpers sit on top of the Java bridge rather than owning another wire codec.
