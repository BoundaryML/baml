import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const nodeRuntime = resolve(packageRoot, "../bridge_typescript/typescript_src");
const shared = resolve(packageRoot, "typescript_src/shared");
const distProto = resolve(packageRoot, "dist/shared/proto");
const files = ["define_function.ts", "errors.ts", "host_value_registry.ts", "proto.ts", "stream.ts", "typemap.ts", "wire_ty.ts"];

rmSync(shared, { recursive: true, force: true });
mkdirSync(shared, { recursive: true });
for (const file of files) {
  const source = readFileSync(resolve(nodeRuntime, file), "utf8")
    .replaceAll("from './native.js'", "from '../native.js'")
    .replaceAll("Buffer", "Uint8Array");
  writeFileSync(resolve(shared, file), source);
}
cpSync(resolve(nodeRuntime, "proto"), resolve(shared, "proto"), { recursive: true });
rmSync(distProto, { recursive: true, force: true });
cpSync(resolve(nodeRuntime, "proto"), distProto, { recursive: true });
for (const directory of [resolve(shared, "proto"), distProto]) {
  const protoPath = resolve(directory, "baml_cffi.js");
  const proto = readFileSync(protoPath, "utf8").replace('import $protobuf from "protobufjs/minimal.js";', 'import * as $protobuf from "protobufjs/minimal.js";');
  writeFileSync(protoPath, proto);
}
await build({
  entryPoints: [resolve(shared, "proto/baml_cffi.js")],
  outfile: resolve(distProto, "baml_cffi.js"),
  bundle: true,
  format: "esm",
  platform: "browser",
});
