import type { ReleaseTrack, TrackMetadata } from "./types"

const anchorMarker = /^\s*\/\/\s*ANCHOR(_END)?:\s*(\S(?:.*\S)?)\s*$/
const controlMarker = /^\s*\/\/\s*(TRACK|NO_TRACK|NO_TRACK_REASON):\s*(.*?)\s*$/i
const tracks = new Set<ReleaseTrack>(["canary", "nightly", "head"])

export interface ExtractedAnchor {
  code: string
  sourceBefore: string
  sourceAfter: string
}

export function extractAnchor(source: string, requestedAnchor: string): ExtractedAnchor {
  const lines = source.replace(/\r\n/g, "\n").split("\n")
  const regions = new Map<string, { start: number; end: number }>()
  let open: { name: string; line: number } | undefined

  for (const [index, line] of lines.entries()) {
    const marker = line.match(anchorMarker)
    if (!marker) continue

    const isEnd = Boolean(marker[1])
    const name = marker[2]
    if (!name) continue
    if (!isEnd) {
      if (open) {
        throw new Error(`Nested anchor '${name}' inside '${open.name}' at line ${index + 1}`)
      }
      if (regions.has(name)) throw new Error(`Duplicate anchor '${name}'`)
      open = { name, line: index }
      continue
    }

    if (!open) throw new Error(`Anchor '${name}' ends without a matching start at line ${index + 1}`)
    if (open.name !== name) {
      throw new Error(`Anchor '${open.name}' is closed by '${name}' at line ${index + 1}`)
    }
    regions.set(name, { start: open.line, end: index })
    open = undefined
  }

  if (open) throw new Error(`Anchor '${open.name}' has no matching end`)
  const region = regions.get(requestedAnchor)
  if (!region) throw new Error(`Missing anchor '${requestedAnchor}'`)

  const visibleLines = lines
    .slice(region.start + 1, region.end)
    .filter((line) => !controlMarker.test(line) && !anchorMarker.test(line))
  const code = visibleLines.join("\n").trimEnd()
  if (!code.trim()) throw new Error(`Anchor '${requestedAnchor}' is empty`)

  return {
    code,
    sourceBefore: `${lines.slice(0, region.start + 1).join("\n")}\n`,
    sourceAfter: `\n${lines.slice(region.end).join("\n")}`,
  }
}

function releaseTrack(value: string, key: string): ReleaseTrack {
  const normalized = value.trim().toLowerCase() as ReleaseTrack
  if (!tracks.has(normalized)) {
    throw new Error(`${key} must be one of: canary, nightly, head`)
  }
  return normalized
}

export function parseTrackMetadata(source: string): TrackMetadata {
  const values = new Map<string, string>()

  for (const line of source.replace(/\r\n/g, "\n").split("\n")) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith("//")) {
      const marker = line.match(controlMarker)
      if (!marker) continue
      const key = marker[1]?.toUpperCase()
      const value = marker[2]
      if (!key || value === undefined) continue
      if (values.has(key)) throw new Error(`Duplicate ${key} declaration`)
      values.set(key, value.trim())
      continue
    }
    break
  }

  const declaredTrack = values.has("TRACK") ? releaseTrack(values.get("TRACK")!, "TRACK") : "canary"
  const excludedTrack = values.has("NO_TRACK") ? releaseTrack(values.get("NO_TRACK")!, "NO_TRACK") : undefined
  const exclusionReason = values.get("NO_TRACK_REASON")?.trim()

  if (exclusionReason && !excludedTrack) throw new Error("NO_TRACK_REASON requires NO_TRACK")
  if (excludedTrack && !exclusionReason) throw new Error("NO_TRACK requires a non-empty NO_TRACK_REASON")
  if (excludedTrack === declaredTrack) throw new Error("A fixture cannot exclude its declared TRACK")

  return { declaredTrack, excludedTrack, exclusionReason }
}
