# Regenerating proto clients for baml_inbound.proto / baml_outbound.proto

```sh
# Rust (prost) + Python (protoc) — both driven by bridge_ctypes/build.rs
cargo build -p bridge_ctypes
#   -> target/.../build/bridge_ctypes-*/out/baml_core.cffi.v1.rs
#   -> sdks/python/src/baml_core/cffi/v1/*_pb2.py(i)

# Node / TypeScript (protobufjs)
cd crates/bridge_nodejs && pnpm build:debug    # or: pnpm build:proto
#   -> crates/bridge_nodejs/typescript_src/proto/baml_cffi.{js,d.ts}

# TypeScript (ts-proto / buf) — typescript2/pkg-proto consumer
cd typescript2/pkg-proto && pnpm generate
#   -> typescript2/pkg-proto/src/generated/baml_core/cffi/v1/*.ts

# Go (protoc-gen-go)
cd crates/bridge_go && ./build.sh
#   -> crates/bridge_go/cffi/proto/baml_core/cffi/v1/*.pb.go
```

Other consumers (`bridge_cffi`, `bridge_wasm`, `sdks/python/rust/bridge_python`) use the Rust prost types via `bridge_ctypes` — nothing extra to regenerate.

No clients exist for Ruby, Java/Kotlin, C#/.NET, Swift, or PHP.
