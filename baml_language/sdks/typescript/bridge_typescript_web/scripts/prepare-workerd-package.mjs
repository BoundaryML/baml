import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = resolve(packageRoot, "dist");
const workerd = resolve(dist, "workerd");
const workerdWasm = resolve(dist, "workerd-wasm");

rmSync(workerd, { recursive: true, force: true });
mkdirSync(workerd, { recursive: true });

for (const file of ["index.js", "index.js.map", "index.d.ts", "index.d.ts.map", "native.d.ts", "native.d.ts.map"]) {
  cpSync(resolve(dist, file), resolve(workerd, file));
}
cpSync(resolve(dist, "shared"), resolve(workerd, "shared"), { recursive: true });

const workerdProtoPath = resolve(workerd, "shared/proto/baml_cffi.js");
const workerdProto = readFileSync(workerdProtoPath, "utf8");
const protobufImport = "var $protobuf = __toESM(require_minimal2(), 1);";
const workerdSafeProto = workerdProto.replace(protobufImport, `${protobufImport}
$protobuf.util.Buffer = null;
$protobuf.configure();`);
if (workerdSafeProto === workerdProto) {
  throw new Error("could not select protobufjs's Uint8Array reader/writer for workerd");
}
writeFileSync(workerdProtoPath, workerdSafeProto);

const browserCoreImport = 'from "./wasm/bridge_web_core.js";';
const workerdCoreImport = 'from "../workerd-wasm/bridge_web_core.js";';
const browserNative = readFileSync(resolve(dist, "native.js"), "utf8");
const workerdNative = browserNative
  .replace("import initWasm, {", "import {")
  .replace(browserCoreImport, workerdCoreImport)
  .replace("await initWasm();", "")
  .replace("//# sourceMappingURL=native.js.map", "");

if (workerdNative === browserNative || !workerdNative.includes(workerdCoreImport)) {
  throw new Error("could not prepare the workerd WASM loader");
}
writeFileSync(resolve(workerd, "native.js"), workerdNative);

const workerBuildEntrypoint = resolve(workerdWasm, "index.js");
const workerBuildSource = readFileSync(workerBuildEntrypoint, "utf8");
for (const method of ["stageRuntimeBytecode", "callFunctionSync", "callFunction", "newFunctionCall", "cancelFunctionCall"]) {
  if (!workerBuildSource.includes(`prototype.${method}`)) {
    throw new Error(`worker-build entrypoint does not expose ${method}`);
  }
}
cpSync(resolve(packageRoot, "typescript_src/wasm/bridge_web_core.d.ts"), resolve(workerdWasm, "bridge_web_core.d.ts"));

// worker-build turns non-event Wasm exports into WorkerEntrypoint prototype
// methods. Re-export those methods to retain the bridge's library-shaped API.
writeFileSync(resolve(workerdWasm, "bridge_web_core.js"), `
import WorkerBridgeEntrypoint from "./index.js";

const bridge = WorkerBridgeEntrypoint.prototype;

export const stageRuntimeBytecode = bridge.stageRuntimeBytecode;
export const callFunctionSync = bridge.callFunctionSync;
export const callFunction = bridge.callFunction;
export const newFunctionCall = bridge.newFunctionCall;
export const cancelFunctionCall = bridge.cancelFunctionCall;

export default async function initWasm() {}
`);
