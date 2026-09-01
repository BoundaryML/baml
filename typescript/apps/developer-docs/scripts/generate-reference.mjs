import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const repoRoot = path.resolve(appRoot, "../../..")
const cargoManifest = path.join(repoRoot, "baml_language/Cargo.toml")
const binary = path.join(repoRoot, "baml_language/target/debug/baml-cli")
const checkOnly = process.argv.includes("--check")

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, ...options })
  if (result.status !== 0) {
    if (result.stdout) process.stdout.write(result.stdout)
    if (result.stderr) process.stderr.write(result.stderr)
    process.exit(result.status ?? 1)
  }
  return result.stdout.trim()
}

run("cargo", ["build", "--manifest-path", cargoManifest, "-p", "baml_cli"])

const rawPackage = run(binary, ["describe", "baml", "--export"])
const packageExport = JSON.parse(rawPackage)
if (packageExport.format_version !== 1) {
  throw new Error(`Unsupported baml describe format_version: ${packageExport.format_version}`)
}

const versionOutput = run(binary, ["--version"])
const toolchainVersion = versionOutput.split(/\s+/).at(-1)
if (!toolchainVersion) throw new Error(`Could not parse toolchain version from '${versionOutput}'`)
const packageName = packageExport.package
const outputPath = path.join(appRoot, `reference-data/releases/${toolchainVersion}/${packageName}.v1.json`)
const manifestPath = path.join(appRoot, "reference-data/channels.v1.json")

// A tree object is available in shallow CI checkouts and changes iff the
// language source changes, unlike a path-filtered commit lookup at a shallow
// history boundary.
const sourceRevision = run("git", ["rev-parse", "HEAD:baml_language"])
const artifactDigest = createHash("sha256").update(rawPackage).digest("hex")
function compactRecord(record, kind, namespace, memberCount = 0, display) {
  return {
    id: record.id,
    kind,
    name: record.name,
    ...(namespace.length ? { namespace } : {}),
    ...(record.docstring ? { summary: record.docstring.split("\n")[0] } : {}),
    ...(record.synthetic ? { synthetic: true } : {}),
    ...(record.signature ? { signature: record.signature } : {}),
    ...(display ? { display } : {}),
    member_count: memberCount,
  }
}

const catalogItems = packageExport.items.flatMap((item) => {
  const namespace = item.namespace ?? []
  const memberNamespace = [...namespace, item.name]
  const methods = [
    ...(item.methods ?? []),
    ...(item.required_methods ?? []),
    ...(item.default_methods ?? []),
  ].map((member) => compactRecord(member, "method", memberNamespace))
  const fields = (item.fields ?? []).map((member) =>
    compactRecord(member, "field", memberNamespace, 0, `${member.name}: ${member.ty.display}`),
  )
  const variants = (item.variants ?? []).map((member) => compactRecord(member, "variant", memberNamespace))
  const associatedTypes = (item.assoc_types ?? []).map((member) =>
    compactRecord(member, "associated_type", memberNamespace, 0, member.default ? `${member.name} = ${member.default.display}` : member.name),
  )
  const memberCount = methods.length + fields.length + variants.length + associatedTypes.length
  const itemDisplay = item.resolved ? `${item.name} = ${item.resolved.display}` : undefined
  return [compactRecord(item, item.kind, namespace, memberCount, itemDisplay), ...methods, ...fields, ...variants, ...associatedTypes]
})
const catalog = {
  package: packageExport.package,
  source_item_count: packageExport.items.length,
  source_impl_count: packageExport.impls.length,
  catalog_item_count: catalogItems.length,
  items: catalogItems,
}
const catalogDigest = createHash("sha256").update(JSON.stringify(catalog, null, 2)).digest("hex")
const document = {
  dataset_schema_version: 1,
  id: `${packageName}-${toolchainVersion}-${artifactDigest.slice(0, 12)}`,
  release: {
    track: "canary",
    toolchain_version: toolchainVersion,
    source_revision: sourceRevision,
    artifact_digest: `sha256:${artifactDigest}`,
    catalog_digest: `sha256:${catalogDigest}`,
  },
  producer: {
    command: "baml describe baml --export",
    format_version: packageExport.format_version,
  },
  describe: packageExport,
  catalog,
}
const serialized = `${JSON.stringify(document, null, 2)}\n`
const channelManifest = {
  schema_version: 1,
  channels: {
    latest: {
      version: toolchainVersion,
      track: "canary",
    },
  },
  releases: [
    {
      version: toolchainVersion,
      packages: [packageName],
    },
  ],
}
const serializedManifest = `${JSON.stringify(channelManifest, null, 2)}\n`

if (checkOnly) {
  if (
    !existsSync(outputPath) ||
    readFileSync(outputPath, "utf8") !== serialized ||
    !existsSync(manifestPath) ||
    readFileSync(manifestPath, "utf8") !== serializedManifest
  ) {
    process.stderr.write("Generated BAML reference data is stale. Run `pnpm --filter @baml/developer-docs generate:reference`.\n")
    process.exit(1)
  }
  process.stdout.write(`Reference dataset ${document.id} is current.\n`)
} else {
  mkdirSync(path.dirname(outputPath), { recursive: true })
  writeFileSync(outputPath, serialized)
  writeFileSync(manifestPath, serializedManifest)
  process.stdout.write(`Wrote ${path.relative(repoRoot, outputPath)} (${document.catalog.items.length} symbols).\n`)
}
