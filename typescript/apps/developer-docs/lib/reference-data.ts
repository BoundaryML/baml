import { readFile } from "node:fs/promises"
import path from "node:path"

export interface ReferenceItem {
  id: string
  kind: string
  name: string
  namespace?: string[]
  summary?: string
  synthetic?: boolean
  display?: string
  signature?: {
    generics?: Array<{ name: string; bounds: string[] }>
    params: Array<{ name: string; ty: { display: string }; optional?: boolean }>
    returns: { display: string }
    throws: { display: string }
  }
  member_count: number
}

export interface DescribeSource {
  file: string
  start: number
  end: number
}

export interface DescribeType {
  display: string
  head?: string
  unresolved?: boolean
}

export interface DescribeGeneric {
  name: string
  bounds: string[]
}

export interface DescribeSignature {
  generics?: DescribeGeneric[]
  params: Array<{ name: string; ty: DescribeType; optional?: boolean }>
  returns: DescribeType
  throws: DescribeType
}

export interface DescribeRecord {
  id: string
  name: string
  docstring?: string
  signature?: DescribeSignature
  source?: DescribeSource
  synthetic?: boolean
  ty?: DescribeType
  default?: DescribeType
}

export interface DescribeItem extends DescribeRecord {
  kind: string
  namespace?: string[]
  detail?: string
  generics?: DescribeGeneric[]
  resolved?: DescribeType
  fields?: DescribeRecord[]
  methods?: DescribeRecord[]
  required_methods?: DescribeRecord[]
  default_methods?: DescribeRecord[]
  variants?: DescribeRecord[]
  assoc_types?: DescribeRecord[]
  implementors?: string[]
  impls?: string[]
}

export interface DescribeImpl {
  id: string
  docstring?: string
  for_ty: DescribeType
  interface?: string
  interface_id?: string
  interface_args?: DescribeType[]
  generics?: DescribeGeneric[]
  assoc_bindings?: Array<{ name: string; ty: DescribeType }>
  methods?: DescribeRecord[]
  source?: DescribeSource
}

export interface DescribeExport {
  format_version: number
  package: string
  items: DescribeItem[]
  impls: DescribeImpl[]
}

export interface ReferenceDataset {
  dataset_schema_version: number
  id: string
  release: {
    track: "canary" | "nightly" | "head"
    toolchain_version: string
    source_revision: string
    artifact_digest: string
    catalog_digest: string
  }
  producer: { command: string; format_version: number }
  describe: DescribeExport
  catalog: {
    package: string
    source_item_count: number
    source_impl_count: number
    catalog_item_count: number
    items: ReferenceItem[]
  }
}

interface ChannelManifest {
  schema_version: number
  channels: Record<string, { version: string; track: ReferenceDataset["release"]["track"] }>
  releases: Array<{ version: string; packages: string[] }>
}

export interface ResolvedPackageReference {
  dataset: ReferenceDataset
  requestedSelector: string
  resolvedVersion: string
  channel?: string
}

const dataRoot = path.join(process.cwd(), "reference-data")

async function readJson<T>(file: string): Promise<T | null> {
  try {
    return JSON.parse(await readFile(file, "utf8")) as T
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return null
    throw error
  }
}

export async function loadReferenceManifest() {
  const manifest = await readJson<ChannelManifest>(path.join(dataRoot, "channels.v1.json"))
  if (!manifest || manifest.schema_version !== 1) throw new Error("Unsupported or missing package-reference channel manifest")
  return manifest
}

export async function availableReferenceSelectors() {
  const manifest = await loadReferenceManifest()
  return [
    ...Object.entries(manifest.channels).map(([selector, channel]) => ({ selector, version: channel.version, track: channel.track, channel: true })),
    ...manifest.releases.map((release) => ({ selector: release.version, version: release.version, track: undefined, channel: false })),
  ]
}

export async function loadPackageReference(selector: string, packageName: string): Promise<ResolvedPackageReference | null> {
  const manifest = await loadReferenceManifest()
  const channel = manifest.channels[selector]
  const resolvedVersion = channel?.version ?? selector
  const release = manifest.releases.find((candidate) => candidate.version === resolvedVersion)
  if (!release?.packages.includes(packageName)) return null

  const dataset = await readJson<ReferenceDataset>(path.join(dataRoot, "releases", resolvedVersion, `${packageName}.v1.json`))
  if (!dataset) return null
  if (dataset.dataset_schema_version !== 1 || dataset.producer.format_version !== 1 || dataset.describe.format_version !== 1) throw new Error(`Unsupported package-reference dataset '${dataset.id}'`)
  if (dataset.catalog.package !== packageName || dataset.describe.package !== packageName || dataset.release.toolchain_version !== resolvedVersion) throw new Error(`Package-reference manifest does not match dataset '${dataset.id}'`)

  return { dataset, requestedSelector: selector, resolvedVersion, ...(channel ? { channel: selector } : {}) }
}

export function isPublicReferenceItem(item: ReferenceItem) {
  return !item.synthetic && !item.name.startsWith("_") && !item.summary?.toLowerCase().startsWith("(internal)")
}

export function referenceItemSegments(item: ReferenceItem) {
  return [...(item.namespace ?? []), item.name]
}

export function referenceItemName(item: ReferenceItem, packageName: string) {
  return [packageName, ...referenceItemSegments(item)].join(".")
}

export function referenceItemPath(selector: string, packageName: string, item: ReferenceItem) {
  return `/baml/packages/${selector}/${packageName}/${referenceItemSegments(item).map(encodeURIComponent).join("/")}`
}

export function referencePackagePath(selector: string, packageName: string) {
  return `/baml/packages/${encodeURIComponent(selector)}/${encodeURIComponent(packageName)}`
}

export function findReferenceItem(dataset: ReferenceDataset, segments: string[]) {
  return dataset.catalog.items.find((item) => {
    const candidate = referenceItemSegments(item)
    return candidate.length === segments.length && candidate.every((segment, index) => segment === segments[index])
  })
}

export function directReferenceMembers(dataset: ReferenceDataset, item: ReferenceItem) {
  const namespace = referenceItemSegments(item)
  return dataset.catalog.items.filter((candidate) => {
    const candidateNamespace = candidate.namespace ?? []
    return isPublicReferenceItem(candidate) && candidateNamespace.length === namespace.length && candidateNamespace.every((segment, index) => segment === namespace[index])
  })
}

const describeMemberCollections = ["fields", "methods", "required_methods", "default_methods", "variants", "assoc_types"] as const

export function findDescribeItem(dataset: ReferenceDataset, id: string) {
  return dataset.describe.items.find((item) => item.id === id)
}

export function findDescribeRecord(dataset: ReferenceDataset, id: string): DescribeItem | DescribeRecord | undefined {
  const item = findDescribeItem(dataset, id)
  if (item) return item
  for (const parent of dataset.describe.items) {
    for (const collection of describeMemberCollections) {
      const member = parent[collection]?.find((candidate) => candidate.id === id)
      if (member) return member
    }
  }
  for (const implementation of dataset.describe.impls) {
    const method = implementation.methods?.find((candidate) => candidate.id === id)
    if (method) return method
  }
}

export function describeImplementations(dataset: ReferenceDataset, item: DescribeItem) {
  const ids = new Set(item.impls ?? [])
  return dataset.describe.impls.filter((implementation) => ids.has(implementation.id))
}

export function formatReferenceSignature(item: ReferenceItem) {
  if (item.display) return item.display
  if (!item.signature) return undefined
  const generics = item.signature.generics?.length ? `<${item.signature.generics.map((generic) => generic.name).join(", ")}>` : ""
  const params = item.signature.params.map((param) => `${param.name}${param.optional ? "?" : ""}: ${param.ty.display}`).join(", ")
  const throws = item.signature.throws.display === "never" ? "" : ` throws ${item.signature.throws.display}`
  return `${item.name}${generics}(${params}) -> ${item.signature.returns.display}${throws}`
}
