/// <reference lib="WebWorker" />

interface RuntimeRequest {
  requestId: number
  action: "check" | "run"
  files: Record<string, string>
  functionName?: string
  args: Record<string, unknown>
}

interface Diagnostic {
  severity: "error"
  name: string
  message: string
  className?: string
  trace?: string[]
}

function diagnostic(error: unknown): Diagnostic {
  const value = error as Error & { className?: string; bamlTrace?: string[] }
  return {
    severity: "error",
    name: value?.name || "Error",
    message: value?.message || String(error),
    className: value?.className,
    trace: value?.bamlTrace,
  }
}

function displayResult(value: unknown) {
  if (typeof value === "string") return value
  return JSON.stringify(value, (_key, nested) => typeof nested === "bigint" ? nested.toString() : nested, 2)
}

const scope = self as DedicatedWorkerGlobalScope

scope.onmessage = async ({ data }: MessageEvent<RuntimeRequest>) => {
  try {
    const { BamlRuntime, callFunction } = await import("@boundaryml/baml-bridge-web")
    const runtimeSources = Object.fromEntries(Object.entries(data.files).filter(([file]) => file.endsWith(".baml")))
    const runtime = BamlRuntime.initializeRuntime("/workspace", runtimeSources)
    if (data.action === "check") {
      scope.postMessage({ requestId: data.requestId, ok: true, output: "Compiled successfully." })
      return
    }
    if (!data.functionName) throw new Error("This fixture does not declare a runnable function.")
    const result = await callFunction(runtime, data.functionName, data.args)
    scope.postMessage({ requestId: data.requestId, ok: true, output: displayResult(result.result()) })
  } catch (error) {
    scope.postMessage({ requestId: data.requestId, ok: false, diagnostics: [diagnostic(error)] })
  }
}

export {}
