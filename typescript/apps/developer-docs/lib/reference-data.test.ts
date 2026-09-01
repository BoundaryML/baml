import { createHash } from "node:crypto"
import { readFileSync } from "node:fs"
import path from "node:path"
import { describe, expect, it } from "vitest"

import { findDescribeItem, findReferenceItem, referenceItemPath } from "./reference-data"

const manifest = JSON.parse(readFileSync(path.join(process.cwd(), "reference-data/channels.v1.json"), "utf8"))
const exactVersion = manifest.channels.latest.version
const dataset = JSON.parse(readFileSync(path.join(process.cwd(), `reference-data/releases/${exactVersion}/baml.v1.json`), "utf8"))

describe("generated BAML package reference", () => {
  it("resolves latest to one exact immutable release", () => {
    expect(manifest.schema_version).toBe(1)
    expect(manifest.channels.latest).toEqual({ version: dataset.release.toolchain_version, track: "canary" })
    expect(manifest.releases).toContainEqual({ version: exactVersion, packages: ["baml"] })
  })

  it("keeps the supported package dataset envelope", () => {
    expect(dataset.dataset_schema_version).toBe(1)
    expect(dataset.producer).toMatchObject({ command: "baml describe baml --export", format_version: 1 })
    expect(dataset.release.track).toBe("canary")
    expect(dataset.release.source_revision).toMatch(/^[0-9a-f]{40}$/)
    expect(dataset.catalog.package).toBe("baml")
    expect(dataset.catalog.source_item_count).toBeGreaterThan(100)
    expect(dataset.catalog.catalog_item_count).toBeGreaterThan(dataset.catalog.source_item_count)
    expect(dataset.catalog.items).toHaveLength(dataset.catalog.catalog_item_count)
  })

  it("routes generated objects by selector, package, and object path", () => {
    const array = findReferenceItem(dataset, ["Array"])
    expect(array).toMatchObject({ id: "T:baml.Array", kind: "class", name: "Array" })
    expect(referenceItemPath("latest", "baml", array!)).toBe("/baml/packages/latest/baml/Array")
    const at = findReferenceItem(dataset, ["Array", "at"])
    expect(at).toMatchObject({ id: "M:baml.Array.at", kind: "method", name: "at" })
    expect(referenceItemPath(exactVersion, "baml", at!)).toBe(`/baml/packages/${exactVersion}/baml/Array/at`)
  })

  it("retains the complete describe payload for rendering", () => {
    expect(dataset.describe.format_version).toBe(1)
    expect(dataset.describe.items).toHaveLength(dataset.catalog.source_item_count)
    expect(dataset.describe.impls).toHaveLength(dataset.catalog.source_impl_count)
    const describedArray = findDescribeItem(dataset, "T:baml.Array")
    expect(describedArray).toMatchObject({
      id: "T:baml.Array",
      kind: "class",
      source: { file: "<builtin>/baml/containers.baml" },
    })
    expect(describedArray?.methods).toHaveLength(33)
    expect(describedArray?.impls?.length).toBeGreaterThan(0)
  })

  it("pins producer and derived catalog digests", () => {
    const catalogDigest = createHash("sha256").update(JSON.stringify(dataset.catalog, null, 2)).digest("hex")
    expect(dataset.release.catalog_digest).toBe(`sha256:${catalogDigest}`)
    expect(dataset.release.artifact_digest).toMatch(/^sha256:[0-9a-f]{64}$/)
    expect(dataset.id).toBe(`baml-${exactVersion}-${dataset.release.artifact_digest.slice(7, 19)}`)
  })
})
