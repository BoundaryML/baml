import "server-only"

import { readFile, readdir } from "node:fs/promises"
import path from "node:path"

import { highlightBaml } from "@/lib/baml-highlight"
import { extractAnchor, parseTrackMetadata } from "./markers"
import { verificationModes, type ExampleManifest, type LoadedBamlExample } from "./types"

const examplesRoot = path.join(process.cwd(), "examples")
const runtimeVersion = "0.18.1-nightly.20260828.a"

function assertManifest(value: unknown, requestedId: string): asserts value is ExampleManifest {
  if (!value || typeof value !== "object") throw new Error(`Example '${requestedId}' has invalid metadata`)
  const manifest = value as Record<string, unknown>
  for (const key of ["id", "title", "caption", "file", "anchor", "mode"] as const) {
    if (typeof manifest[key] !== "string" || !manifest[key]) throw new Error(`Example '${requestedId}' is missing '${key}'`)
  }
  if (manifest.id !== requestedId) throw new Error(`Example metadata ID '${manifest.id}' does not match '${requestedId}'`)
  if (!Number.isInteger(manifest.listing) || Number(manifest.listing) < 1) throw new Error(`Example '${requestedId}' needs a positive listing number`)
  if (!verificationModes.includes(manifest.mode as ExampleManifest["mode"])) throw new Error(`Example '${requestedId}' has an unknown verification mode`)
  if (manifest.mode === "run" && typeof manifest.functionName !== "string") throw new Error(`Runnable example '${requestedId}' needs a functionName`)
  if (manifest.args !== undefined && (!manifest.args || typeof manifest.args !== "object" || Array.isArray(manifest.args))) {
    throw new Error(`Example '${requestedId}' args must be an object`)
  }
}

function safeExampleDirectory(id: string) {
  if (!/^[a-z0-9]+(?:[a-z0-9-]*\/[a-z0-9][a-z0-9-]*)$/.test(id)) throw new Error(`Invalid example ID '${id}'`)
  const directory = path.resolve(examplesRoot, id)
  if (!directory.startsWith(`${examplesRoot}${path.sep}`)) throw new Error(`Example ID '${id}' escapes the fixture root`)
  return directory
}

async function readFixtureFiles(directory: string) {
  const files: Record<string, string> = {}
  async function visit(current: string) {
    for (const entry of await readdir(current, { withFileTypes: true })) {
      const absolute = path.join(current, entry.name)
      if (entry.isDirectory()) await visit(absolute)
      else if (entry.name !== "example.json") files[path.relative(directory, absolute)] = await readFile(absolute, "utf8")
    }
  }
  await visit(directory)
  return files
}

export async function loadBamlExample(id: string): Promise<LoadedBamlExample> {
  const directory = safeExampleDirectory(id)
  const manifest = JSON.parse(await readFile(path.join(directory, "example.json"), "utf8")) as unknown
  assertManifest(manifest, id)
  const files = await readFixtureFiles(directory)
  const primarySource = files[manifest.file]
  if (primarySource === undefined) throw new Error(`Example '${id}' cannot find '${manifest.file}'`)

  const anchor = extractAnchor(primarySource, manifest.anchor)
  const track = parseTrackMetadata(primarySource)
  return {
    ...manifest,
    ...track,
    ...anchor,
    files,
    runtimeVersion,
    highlightedHtml: await highlightBaml(anchor.code),
  }
}
