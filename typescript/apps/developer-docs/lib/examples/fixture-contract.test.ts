import { readFileSync } from "node:fs"
import path from "node:path"
import { describe, expect, it } from "vitest"

import { extractAnchor, parseTrackMetadata } from "./markers"

const fixtureRoot = path.join(process.cwd(), "examples/book/first-function")
const manifest = JSON.parse(readFileSync(path.join(fixtureRoot, "example.json"), "utf8"))
const source = readFileSync(path.join(fixtureRoot, manifest.file), "utf8")

describe("published fixture contract", () => {
  it("derives the listing and track from the real project", () => {
    expect(extractAnchor(source, manifest.anchor).code).toContain("function Welcome")
    expect(parseTrackMetadata(source).declaredTrack).toBe("nightly")
  })

  it("does not allow a hand-authored BAML fence beside fixture-backed listings", () => {
    const chapter = readFileSync(path.join(process.cwd(), "content/docs/baml/book/index.mdx"), "utf8")
    expect(chapter).not.toMatch(/```baml\b/)
    expect(chapter).toContain('<BamlExample id="book/first-function" />')
  })
})
