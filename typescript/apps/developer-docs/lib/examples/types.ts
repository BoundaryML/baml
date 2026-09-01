export const verificationModes = ["check", "test", "run", "expect-fail", "fragment"] as const

export type VerificationMode = (typeof verificationModes)[number]
export type ReleaseTrack = "canary" | "nightly" | "head"

export interface ExampleManifest {
  id: string
  title: string
  listing: number
  caption: string
  file: string
  anchor: string
  mode: VerificationMode
  functionName?: string
  args?: Record<string, unknown>
}

export interface TrackMetadata {
  declaredTrack: ReleaseTrack
  excludedTrack?: ReleaseTrack
  exclusionReason?: string
}

export interface LoadedBamlExample extends ExampleManifest, TrackMetadata {
  files: Record<string, string>
  code: string
  sourceBefore: string
  sourceAfter: string
  runtimeVersion: string
  highlightedHtml: string
}
