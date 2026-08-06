export type { NativeAddon, ToolingBackend, WasmBridge } from './backend.js';
export { NativeBackend, request, WasmBackend } from './backend.js';
export { DiskArtifactCache } from './cache.js';
export {
  buildLineIndex,
  LineIndexCache,
  utf8OffsetToUtf16,
  utf16OffsetToUtf8,
} from './encoding.js';
export {
  assertMapHashes,
  generatedToSource,
  sourceToGenerated,
} from './mapping.js';
export type { LoadProjectOptions } from './node.js';
export { loadProject } from './node.js';
export type { FileAccess, OpenProjectOptions } from './project.js';
export {
  BamlFileDiscovery,
  BamlProject,
  discoverBamlFiles,
  discoverProject,
  moduleSpecifier,
  projectId,
  resolveBamlSpecifier,
} from './project.js';
export type {
  Capabilities,
  CheckResult,
  Location,
  Segment,
  SegmentMap,
  VirtualModule,
  WorkspaceEdit,
} from './protocol.js';
export { isBamlSidecar, sidecarFingerprint, sidecarPath } from './sidecar.js';
