"use client"

import { useEffect, useRef, useState } from "react"
import { Check, Clipboard, Code2, Pencil, Play, RotateCcw, X } from "lucide-react"

import type { LoadedBamlExample } from "@/lib/examples/types"
import { cn } from "@/lib/utils"

type ExampleProps = Pick<
  LoadedBamlExample,
  | "id"
  | "title"
  | "listing"
  | "caption"
  | "file"
  | "mode"
  | "functionName"
  | "args"
  | "declaredTrack"
  | "excludedTrack"
  | "exclusionReason"
  | "files"
  | "code"
  | "sourceBefore"
  | "sourceAfter"
  | "runtimeVersion"
  | "highlightedHtml"
>

interface RuntimeResponse {
  requestId: number
  ok: boolean
  output?: string
  diagnostics?: Array<{ name: string; message: string; className?: string; trace?: string[] }>
}

type RunState =
  | { status: "idle" }
  | { status: "running" }
  | { status: "success"; output: string }
  | { status: "error"; message: string }

export function BamlExampleClient(example: ExampleProps) {
  const [code, setCode] = useState(example.code)
  const [argsText, setArgsText] = useState(JSON.stringify(example.args ?? {}, null, 2))
  const [editing, setEditing] = useState(false)
  const [copied, setCopied] = useState(false)
  const [runState, setRunState] = useState<RunState>({ status: "idle" })
  const workerRef = useRef<Worker | null>(null)
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const requestId = useRef(0)
  const dirty = code !== example.code || argsText !== JSON.stringify(example.args ?? {}, null, 2)

  useEffect(() => () => {
    workerRef.current?.terminate()
    if (timeoutRef.current) clearTimeout(timeoutRef.current)
  }, [])

  function finishWorker() {
    workerRef.current?.terminate()
    workerRef.current = null
    if (timeoutRef.current) clearTimeout(timeoutRef.current)
    timeoutRef.current = null
  }

  async function copyCode() {
    await navigator.clipboard.writeText(code)
    setCopied(true)
    setTimeout(() => setCopied(false), 1200)
  }

  function reset() {
    setCode(example.code)
    setArgsText(JSON.stringify(example.args ?? {}, null, 2))
    setRunState({ status: "idle" })
  }

  function execute() {
    let args: Record<string, unknown> = {}
    try {
      const parsed = JSON.parse(argsText) as unknown
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("Arguments must be a JSON object.")
      args = parsed as Record<string, unknown>
    } catch (error) {
      setRunState({ status: "error", message: error instanceof Error ? error.message : String(error) })
      return
    }

    finishWorker()
    setRunState({ status: "running" })
    const worker = new Worker(new URL("./baml-runtime.worker.ts", import.meta.url), { type: "module" })
    workerRef.current = worker
    const currentRequest = ++requestId.current

    worker.onmessage = ({ data }: MessageEvent<RuntimeResponse>) => {
      if (data.requestId !== currentRequest) return
      finishWorker()
      if (data.ok) setRunState({ status: "success", output: data.output ?? "Completed." })
      else {
        const message = data.diagnostics?.map((item) => [item.message, ...(item.trace ?? [])].join("\n")).join("\n\n") || "BAML could not run this example."
        setRunState({ status: "error", message })
      }
    }
    worker.onerror = (event) => {
      finishWorker()
      setRunState({ status: "error", message: event.message || "The BAML runtime worker failed to load." })
    }
    timeoutRef.current = setTimeout(() => {
      finishWorker()
      setRunState({ status: "error", message: "The example exceeded its 30 second execution limit." })
    }, 30_000)

    worker.postMessage({
      requestId: currentRequest,
      action: example.mode === "run" ? "run" : "check",
      files: { ...example.files, [example.file]: `${example.sourceBefore}${code}${example.sourceAfter}` },
      functionName: example.functionName,
      args,
    })
  }

  const actionLabel = example.mode === "run" ? "Run" : "Check"

  return (
    <figure className="baml-example not-prose my-7 overflow-hidden rounded-xl border bg-code shadow-sm" data-example-id={example.id}>
      <figcaption className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b bg-background/70 px-3 py-2.5 text-sm">
        <div className="min-w-0">
          <div className="truncate font-medium">Listing {example.listing}. {example.title}</div>
          <div className="mt-1 flex min-w-0 items-center gap-2">
            <code className="truncate text-xs text-muted-foreground">{example.file}</code>
            <span className="shrink-0 rounded-full border px-2 py-0.5 text-[10px] font-medium uppercase tracking-wide text-muted-foreground">{example.declaredTrack}</span>
          </div>
        </div>
        <div className="flex items-center gap-1">
          <button aria-label={editing ? "Finish editing" : "Edit code"} className="baml-example-button" onClick={() => setEditing((value) => !value)} title={editing ? "Finish editing" : "Edit code"} type="button">
            {editing ? <X /> : <Pencil />}<span className="sr-only">{editing ? "Done" : "Edit"}</span>
          </button>
          <button aria-label="Copy code" className="baml-example-button" onClick={copyCode} title="Copy code" type="button">
            {copied ? <Check /> : <Clipboard />}<span className="sr-only">{copied ? "Copied" : "Copy"}</span>
          </button>
          {dirty ? (
            <button aria-label="Reset example" className="baml-example-button" onClick={reset} title="Reset example" type="button">
              <RotateCcw /><span className="sr-only">Reset</span>
            </button>
          ) : null}
          <button className="baml-example-button bg-foreground text-background hover:bg-foreground/85" disabled={runState.status === "running"} onClick={execute} type="button">
            {example.mode === "run" ? <Play /> : <Code2 />}<span>{runState.status === "running" ? `${actionLabel}ning…` : actionLabel}</span>
          </button>
        </div>
      </figcaption>

      {editing ? (
        <textarea
          aria-label={`${example.title} source`}
          className="min-h-44 w-full resize-y bg-transparent p-4 font-mono text-[13px] leading-6 outline-none"
          onChange={(event) => setCode(event.target.value)}
          spellCheck={false}
          value={code}
        />
      ) : dirty ? (
        <pre className="m-0 overflow-x-auto bg-transparent p-4 text-[13px] leading-6"><code>{code}</code></pre>
      ) : (
        <div className="baml-example-highlight" dangerouslySetInnerHTML={{ __html: example.highlightedHtml }} />
      )}

      {example.mode === "run" ? (
        <label className="grid gap-1.5 border-t bg-background/45 px-3 py-2.5 text-xs font-medium text-muted-foreground">
          Arguments
          <textarea
            aria-label={`${example.functionName} arguments`}
            className="min-h-14 resize-y rounded-md border bg-background px-3 py-2 font-mono text-xs leading-5 text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
            onChange={(event) => setArgsText(event.target.value)}
            spellCheck={false}
            value={argsText}
          />
        </label>
      ) : null}

      {runState.status !== "idle" ? (
        <div className={cn("border-t px-4 py-3 font-mono text-xs leading-5", runState.status === "error" ? "bg-destructive/8 text-destructive" : "bg-background/65")} role="status">
          {runState.status === "running" ? "Loading the compiler and running this fixture…" : null}
          {runState.status === "success" ? <pre className="m-0 whitespace-pre-wrap bg-transparent p-0 text-inherit">{runState.output}</pre> : null}
          {runState.status === "error" ? <pre className="m-0 whitespace-pre-wrap bg-transparent p-0 text-inherit">{runState.message}</pre> : null}
        </div>
      ) : null}

      {example.excludedTrack && example.exclusionReason ? (
        <div className="border-t bg-amber-500/10 px-4 py-3 text-xs text-amber-900 dark:text-amber-200">
          <strong>Changing soon:</strong> incompatible with {example.excludedTrack} — {example.exclusionReason}
        </div>
      ) : null}
      <div className="flex flex-wrap items-center justify-between gap-2 border-t bg-background/70 px-3 py-2 text-xs text-muted-foreground">
        <span>{example.caption}</span>
        <span className="font-mono">BAML {example.runtimeVersion} · {example.declaredTrack}</span>
      </div>
    </figure>
  )
}
