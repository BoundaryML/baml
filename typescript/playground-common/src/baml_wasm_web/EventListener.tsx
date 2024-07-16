import 'react18-json-view/src/style.css'
// import * as vscode from 'vscode'

import { VSCodeButton } from '@vscode/webview-ui-toolkit/react'
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai'
import { atomFamily, atomWithStorage, unwrap, useAtomCallback } from 'jotai/utils'
import { AlertTriangle, CheckCircle, XCircle } from 'lucide-react'
import { useCallback, useEffect } from 'react'
import CustomErrorBoundary from '../utils/ErrorFallback'
import { atomStore, sessionStore, vscodeLocalStorageStore } from './JotaiProvider'
import { availableProjectsAtom, projectFamilyAtom, projectFilesAtom, runtimeFamilyAtom } from './baseAtoms'
import type {
  WasmDiagnosticError,
  WasmParam,
  WasmRuntime,
  WasmScope,
} from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import { vscode } from '../utils/vscode'
import { v4 as uuid } from 'uuid'

// const wasm = await import("@gloo-ai/baml-schema-wasm-web/baml_schema_build");
// const { WasmProject, WasmRuntime, WasmRuntimeContext, version: RuntimeVersion } = wasm;
const postMessageToExtension = (message: any) => {
  console.log(`Sending message to extension ${message.command}`)
  vscode.postMessage(message)
}

const wasmAtomAsync = atom(async () => {
  const wasm = await import('@gloo-ai/baml-schema-wasm-web/baml_schema_build')
  return wasm
})

const wasmAtom = unwrap(wasmAtomAsync)

const defaultEnvKeyValues: [string, string][] = (() => {
  if ((window as any).next?.version) {
    console.log('Running in nextjs')

    const domain = window?.location?.origin || ''
    if (domain.includes('localhost')) {
      // we can do somehting fancier here later if we want to test locally.
      return [['BOUNDARY_PROXY_URL', 'https://fiddle-proxy.fly.dev']]
    }
    return [['BOUNDARY_PROXY_URL', 'https://fiddle-proxy.fly.dev']]
  } else {
    console.log('Not running in a Next.js environment, set default value')
    // Not running in a Next.js environment, set default value
    return [['BOUNDARY_PROXY_URL', 'http://localhost:0000']]
  }
})()

const selectedProjectStorageAtom = atomWithStorage<string | null>('selected-project', null, sessionStore)
const selectedFunctionStorageAtom = atom<string | null>(null)
const envKeyValueStorage = atomWithStorage<[string, string][]>(
  'env-key-values',
  defaultEnvKeyValues,
  vscodeLocalStorageStore,
)

export const resetEnvKeyValuesAtom = atom(null, (get, set) => {
  set(envKeyValueStorage, [])
})
export const envKeyValuesAtom = atom(
  (get) => {
    return get(envKeyValueStorage).map(([k, v], idx): [string, string, number] => [k, v, idx])
  },
  (
    get,
    set,
    update: // Update value
    | { itemIndex: number; value: string }
      // Update key
      | { itemIndex: number; newKey: string }
      // Remove key
      | { itemIndex: number; remove: true }
      // Insert key
      | {
          itemIndex: null
          key: string
          value?: string
        },
  ) => {
    if (update.itemIndex !== null) {
      const keyValues = [...get(envKeyValueStorage)]
      if ('value' in update) {
        keyValues[update.itemIndex][1] = update.value
      } else if ('newKey' in update) {
        keyValues[update.itemIndex][0] = update.newKey
      } else if ('remove' in update) {
        keyValues.splice(update.itemIndex, 1)
      }
      console.log('Setting env key values', keyValues)
      set(envKeyValueStorage, keyValues)
    } else {
      set(envKeyValueStorage, (prev) => [...prev, [update.key, update.value ?? '']])
    }
  },
)

type Selection = {
  project?: string
  function?: string
  testCase?: string
}

export const envVarsAtom = atom((get) => {
  const envKeyValues = get(envKeyValuesAtom)
  return Object.fromEntries(envKeyValues.map(([k, v]) => [k, v]))
})

const selectedProjectAtom = atom(
  (get) => {
    const allProjects = get(availableProjectsAtom)
    const project = get(selectedProjectStorageAtom)
    const match = allProjects.find((p) => p === project) ?? allProjects.at(0) ?? null
    return match
  },
  (get, set, project: string) => {
    if (project !== null) {
      set(selectedProjectStorageAtom, project)
    }
  },
)

export const selectedFunctionAtom = atom(
  (get) => {
    const functions = get(availableFunctionsAtom)
    const func = get(selectedFunctionStorageAtom)
    const match = functions.find((f) => f.name === func) ?? functions.at(0)
    return match ?? null
  },
  (get, set, func: string) => {
    if (func !== null) {
      const functions = get(availableFunctionsAtom)
      if (functions.find((f) => f.name === func)) {
        set(selectedFunctionStorageAtom, func)
      }
    }
  },
)

const rawSelectedTestCaseAtom = atom<string | null>(null)
export const selectedTestCaseAtom = atom(
  (get) => {
    const func = get(selectedFunctionAtom)
    const testCases = func?.test_cases ?? []
    const testCase = get(rawSelectedTestCaseAtom)
    const match = testCases.find((tc) => tc.name === testCase) ?? testCases.at(0)
    return match ?? null
  },
  (get, set, testCase: string) => {
    set(rawSelectedTestCaseAtom, testCase)
  },
)

const updateCursorAtom = atom(
  null,
  (get, set, cursor: { fileName: string; fileText: string; line: number; column: number }) => {
    const selectedProject = get(selectedProjectAtom)
    if (selectedProject === null) {
      return
    }

    const project = get(projectFamilyAtom(selectedProject))
    const runtime = get(selectedRuntimeAtom)

    if (runtime && project) {
      const fileName = cursor.fileName
      const fileContent = cursor.fileText
      const lines = fileContent.split('\n')

      let cursorIdx = 0
      for (let i = 0; i < cursor.line - 1; i++) {
        cursorIdx += lines[i].length + 1 // +1 for the newline character
      }

      cursorIdx += cursor.column

      const selectedFunc = runtime.get_function_at_position(fileName, cursorIdx)

      if (selectedFunc) {
        set(selectedFunctionAtom, selectedFunc.name)
      }
    }
  },
)

const removeProjectAtom = atom(null, (get, set, root_path: string) => {
  set(projectFilesAtom(root_path), {})
  set(projectFamilyAtom(root_path), null)
  set(runtimeFamilyAtom(root_path), {})
  const availableProjects = get(availableProjectsAtom)
  set(
    availableProjectsAtom,
    availableProjects.filter((p) => p !== root_path),
  )
})

type WriteFileParams = {
  reason: string
  root_path: string
  files: { name: string; content: string | undefined }[]
} & (
  | {
      replace_all?: true
    }
  | {
      renames?: { from: string; to: string }[]
    }
)

export const updateFileAtom = atom(null, (get, set, params: WriteFileParams) => {
  const { reason, root_path, files } = params
  const replace_all = 'replace_all' in params
  const renames = 'renames' in params ? params.renames ?? [] : []
  console.debug(
    `updateFile: Updating files due to ${reason}: ${files.length} files (${replace_all ? 'replace all' : 'update'})`,
  )
  const _projFiles = get(projectFilesAtom(root_path))
  const filesToDelete = files.filter((f) => f.content === undefined).map((f) => f.name)

  let projFiles = {
    ..._projFiles,
  }
  const filesToModify = files
    .filter((f) => f.content !== undefined)
    .map((f): [string, string] => [f.name, f.content as string])

  renames.forEach(({ from, to }) => {
    if (from in projFiles) {
      projFiles[to] = projFiles[from]
      delete projFiles[from]
      filesToDelete.push(from)
    }
  })

  if (replace_all) {
    for (const file of Object.keys(_projFiles)) {
      if (!filesToDelete.includes(file)) {
        filesToDelete.push(file)
      }
    }
    projFiles = Object.fromEntries(filesToModify)
  }

  let project = get(projectFamilyAtom(root_path))
  const wasm = get(wasmAtom)
  if (project && !replace_all) {
    for (const file of filesToDelete) {
      if (file.startsWith(root_path)) {
        project.update_file(file, undefined)
      }
    }
    console.log('file root path', root_path)
    for (const [name, content] of filesToModify) {
      if (name.startsWith(root_path)) {
        project.update_file(name, content)
        projFiles[name] = content
      }
    }
  } else {
    const onlyRelevantFiles = Object.fromEntries(
      Object.entries(projFiles).filter(([name, _]) => name.startsWith(root_path)),
    )
    // console.log('Creating new project', root_path, onlyRelevantFiles)
    if (wasm) {
      project = wasm.WasmProject.new(root_path, onlyRelevantFiles)
    } else {
      console.log('wasm not yet ready')
    }
  }
  let rt: WasmRuntime | undefined = undefined
  let diag: WasmDiagnosticError | undefined = undefined

  if (project && wasm) {
    try {
      const envVars = get(envVarsAtom)
      rt = project.runtime(envVars)
      diag = project.diagnostics(rt)
    } catch (e) {
      const WasmDiagnosticError = wasm.WasmDiagnosticError
      if (e instanceof Error) {
        console.error(e.message)
      } else if (e instanceof WasmDiagnosticError) {
        diag = e
      } else {
        console.error(e)
      }
    }
  }

  const availableProjects = get(availableProjectsAtom)
  if (!availableProjects.includes(root_path)) {
    set(availableProjectsAtom, [...availableProjects, root_path])
  }

  set(projectFilesAtom(root_path), projFiles)
  set(projectFamilyAtom(root_path), project)
  set(runtimeFamilyAtom(root_path), (prev) => ({
    last_successful_runtime: prev.current_runtime ?? prev.last_successful_runtime,
    current_runtime: rt,
    diagnostics: diag,
  }))
})

export const selectedRuntimeAtom = atom((get) => {
  const project = get(selectedProjectAtom)
  if (!project) {
    return null
  }

  const runtime = get(runtimeFamilyAtom(project))
  if (runtime.current_runtime) return runtime.current_runtime
  if (runtime.last_successful_runtime) return runtime.last_successful_runtime
  return null
})

export const runtimeRequiredEnvVarsAtom = atom((get) => {
  const runtime = get(selectedRuntimeAtom)
  if (!runtime) {
    return []
  }

  return runtime.required_env_vars()
})

const selectedDiagnosticsAtom = atom((get) => {
  const project = get(selectedProjectAtom)
  if (!project) {
    return null
  }

  const runtime = get(runtimeFamilyAtom(project))
  return runtime.diagnostics ?? null
})

export const versionAtom = atom((get) => {
  const wasm = get(wasmAtom)

  if (wasm === undefined) {
    return 'Loading...'
  }

  return wasm.version()
})

export const availableFunctionsAtom = atom((get) => {
  const runtime = get(selectedRuntimeAtom)
  if (!runtime) {
    return []
  }
  return runtime.list_functions()
})

export const streamCurl = atom(true)

const asyncCurlAtom = atom(async (get) => {
  const runtime = get(selectedRuntimeAtom)
  const func = get(selectedFunctionAtom)
  const test_case = get(selectedTestCaseAtom)

  if (!runtime || !func || !test_case) {
    return 'Not yet ready'
  }
  const params = Object.fromEntries(
    test_case.inputs
      .filter((i): i is WasmParam & { value: string } => i.value !== undefined)
      .map((input) => [input.name, JSON.parse(input.value)]),
  )
  try {
    return await func.render_raw_curl(runtime, params, get(streamCurl))
  } catch (e) {
    console.error(e)
    return `${e}`
  }
})

export const curlAtom = unwrap(asyncCurlAtom)

export const renderPromptAtom = atom((get) => {
  const runtime = get(selectedRuntimeAtom)
  const func = get(selectedFunctionAtom)
  const test_case = get(selectedTestCaseAtom)

  if (!runtime || !func || !test_case) {
    return null
  }

  const params = Object.fromEntries(
    test_case.inputs
      .filter((i): i is WasmParam & { value: string } => i.value !== undefined)
      .map((input) => [input.name, JSON.parse(input.value)]),
  )

  try {
    return func.render_prompt(runtime, params)
  } catch (e) {
    if (e instanceof Error) {
      return e.message
    } else {
      return `${e}`
    }
  }
})

export interface TypeCount {
  // options are F (Fallback), R (Retry), D (Direct), B (Round Robin)
  type: string

  // range from 0 to n
  name: number | string
}

const getTypeLetter = (type: string): string => {
  switch (type) {
    case 'Fallback':
      return 'F'
    case 'Retry':
      return 'R'
    case 'Direct':
      return 'D'
    case 'RoundRobin':
      return 'B'
    default:
      return 'U'
  }
}

export interface ClientNode {
  name: string
  node_index: number
  type: string
  identifier: TypeCount[]
  retry_delay?: number

  //necessary for identifying unique round robins, as index matching is not enough
  round_robin_name?: string
}

export interface Edge {
  from_node: string
  to_node: string
  weight?: number
}

export interface NodeEntry {
  gid: ReturnType<typeof uuid>
  weight?: number
}
export interface GroupEntry {
  letter: string
  index: string | number
  client_name?: string
  gid: ReturnType<typeof uuid>
  parentGid?: ReturnType<typeof uuid>
}
const getScopeInfo = (scope: any) => {
  switch (scope.type) {
    case 'Retry':
      return `Index: ${scope.count}, Retry delay: ${scope.delay}`
    case 'RoundRobin':
      return `Index: ${scope.index}, Strategy Name: ${scope.strategy_name}`
    case 'Fallback':
      return `Index: ${scope.index}`
    case 'Direct':
      return ''
    default:
      return 'Unknown scope type'
  }
}
export const orchestration_nodes = atom((get): { nodes: GroupEntry[]; edges: Edge[] } => {
  const func = get(selectedFunctionAtom)
  const runtime = get(selectedRuntimeAtom)

  if (!func || !runtime) {
    return { nodes: [], edges: [] }
  }

  const wasmScopes = func.orchestration_graph(runtime)
  if (wasmScopes === null) {
    return { nodes: [], edges: [] }
  }

  const nodes = createClientNodes(wasmScopes)
  nodes.forEach((node) => {
    node.identifier.unshift({
      type: 'F',
      name: 0,
    })
  })

  nodes.forEach((node) => {
    const stackGroupString = node.identifier.map((item: TypeCount) => `${item.type}${item.name}`).join(' | ')
    console.log(`${stackGroupString}`)
  })

  const { unitNodes, groups } = buildUnitNodesAndGroups(nodes)
  const edges = createEdges(unitNodes)

  const groupArray: GroupEntry[] = Object.values(groups)
  return { nodes: groupArray, edges }
})

function createClientNodes(wasmScopes: any[]): ClientNode[] {
  let indexOuter = 0
  const nodes: ClientNode[] = []

  for (const scope of wasmScopes) {
    const scopeInfo = scope.get_orchestration_scope_info()
    const scopePath = scopeInfo as any[]
    const stackGroup = createStackGroup(scopePath)

    const typeScope = scopePath.length > 2 ? scopePath[scopePath.length - 2] : scopePath[scopePath.length - 1]
    const lastScope = scopePath[scopePath.length - 1]

    const clientNode: ClientNode = {
      name: lastScope.name,
      node_index: indexOuter,
      type: typeScope.type,
      identifier: stackGroup,
    }

    switch (typeScope.type) {
      case 'Retry':
        clientNode.node_index = typeScope.count
        clientNode.retry_delay = typeScope.delay
        break
      case 'RoundRobin':
        clientNode.round_robin_name = typeScope.strategy_name
        break
      case 'Fallback':
        clientNode.type = lastScope.type
        break
      case 'Direct':
        break
      default:
        console.error('Unknown scope type')
    }

    nodes.push(clientNode)
    indexOuter++
  }

  return nodes
}

function createStackGroup(scopePath: any[]): TypeCount[] {
  const stackGroup: TypeCount[] = []

  for (let i = 0; i < scopePath.length; i++) {
    const scope = scopePath[i]
    const indexVal =
      scope.type === 'Retry'
        ? scope.count
        : scope.type === 'Direct'
        ? 0
        : scope.type === 'RoundRobin'
        ? scope.strategy_name
        : scope.index

    stackGroup.push({
      type: getTypeLetter(scope.type),
      name: indexVal,
    })
  }

  return stackGroup
}

function buildUnitNodesAndGroups(nodes: ClientNode[]): {
  unitNodes: NodeEntry[]
  groups: { [gid: string]: GroupEntry }
} {
  const unitNodes: NodeEntry[] = []
  const groups: { [gid: string]: GroupEntry } = {}
  const indexGroups: GroupEntry[] = []

  for (const node of nodes) {
    const stackGroup = node.identifier
    let parentGid = ''

    for (let stackIndex = 0; stackIndex < stackGroup.length; stackIndex++) {
      const scopeLayer = stackGroup[stackIndex]
      const { scopeType, scopeName, curGid } = getScopeDetails(scopeLayer, indexGroups, stackIndex, parentGid)

      if (!(curGid in groups)) {
        groups[curGid] = {
          letter: scopeType,
          index: scopeName,
          client_name: node.name,
          gid: curGid,
          ...(parentGid && { parentGid }),
        }
      }

      indexGroups[stackIndex] = {
        letter: scopeType,
        index: scopeName,
        gid: curGid,
        ...(parentGid && { parentGid }),
      }

      parentGid = curGid
    }

    unitNodes.push({
      gid: parentGid,
      ...(node.type === 'Retry' && { weight: node.retry_delay }),
    })
  }

  return { unitNodes, groups }
}

function getScopeDetails(scopeLayer: TypeCount, indexGroups: GroupEntry[], stackIndex: number, parentGid: string) {
  const scopeType = scopeLayer.type
  const scopeName = scopeLayer.name
  const indexGroupEntry = stackIndex < indexGroups.length ? indexGroups[stackIndex] : null

  let curGid = ''

  if (indexGroupEntry === null) {
    curGid = uuid()
  } else {
    const indexEntryName = indexGroupEntry.index
    const indexEntryGid = indexGroupEntry.gid

    switch (scopeType) {
      case 'R':
        curGid = scopeName >= indexEntryName ? indexEntryGid : uuid()
        break
      case 'F':
        curGid = scopeName == indexEntryName ? indexEntryGid : uuid()
        break
      case 'D':
        curGid = uuid()
        break
      case 'B':
        curGid = scopeName == indexEntryName ? indexEntryGid : uuid()
        break
      default:
        console.error('Unknown scope type')
    }
  }

  return { scopeType, scopeName, curGid }
}

function createEdges(unitNodes: NodeEntry[]): Edge[] {
  return unitNodes.slice(0, -1).map((fromNode, index) => ({
    from_node: fromNode.gid,
    to_node: unitNodes[index + 1].gid,
    ...(fromNode.weight !== null && { weight: fromNode.weight }),
  }))
}

export const diagnositicsAtom = atom((get) => {
  const diagnostics = get(selectedDiagnosticsAtom)
  if (!diagnostics) {
    return []
  }

  return diagnostics.errors()
})

export const numErrorsAtom = atom((get) => {
  const errors = get(diagnositicsAtom)

  const warningCount = errors.filter((e) => e.type === 'warning').length

  return { errors: errors.length - warningCount, warnings: warningCount }
})

const ErrorCount: React.FC = () => {
  const { errors, warnings } = useAtomValue(numErrorsAtom)
  if (errors === 0 && warnings === 0) {
    return (
      <div className="flex flex-row items-center gap-1 text-green-600">
        <CheckCircle size={12} />
      </div>
    )
  }
  if (errors === 0) {
    return (
      <div className="flex flex-row items-center gap-1 text-yellow-600">
        {warnings} <AlertTriangle size={12} />
      </div>
    )
  }
  return (
    <div className="flex flex-row items-center gap-1 text-red-600">
      {errors} <XCircle size={12} /> {warnings} <AlertTriangle size={12} />{' '}
    </div>
  )
}
const createRuntime = (
  wasm: typeof import('@gloo-ai/baml-schema-wasm-web'),
  envVars: Record<string, string>,
  root_path: string,
  project_files: Record<string, string>,
) => {
  const only_project_files = Object.fromEntries(
    Object.entries(project_files).filter(([name, _]) => name.startsWith(root_path)),
  )
  const project = wasm.WasmProject.new(root_path, only_project_files)

  let rt = undefined
  let diag = undefined
  try {
    rt = project.runtime(envVars)
    diag = project.diagnostics(rt)
  } catch (e) {
    const WasmDiagnosticError = wasm.WasmDiagnosticError
    if (e instanceof Error) {
      console.error(e.message)
    } else if (e instanceof WasmDiagnosticError) {
      diag = e
    } else {
      console.error(e)
    }
  }

  return {
    project,
    runtime: rt,
    diagnostics: diag,
  }
}

// We don't use ASTContext.provider because we should the default value of the context
export const EventListener: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const updateFile = useSetAtom(updateFileAtom)
  const updateCursor = useSetAtom(updateCursorAtom)
  const removeProject = useSetAtom(removeProjectAtom)
  const availableProjects = useAtomValue(availableProjectsAtom)
  const [selectedProject, setSelectedProject] = useAtom(selectedProjectAtom)
  const setEnvKeyValueStorage = useSetAtom(envKeyValueStorage)
  const version = useAtomValue(versionAtom)
  const wasm = useAtomValue(wasmAtom)
  const [selectedFunc, setSelectedFunction] = useAtom(selectedFunctionAtom)
  const envVars = useAtomValue(envVarsAtom)

  useEffect(() => {
    if (wasm) {
      console.log('wasm ready!')
      postMessageToExtension({ command: 'get_port' })
      postMessageToExtension({ command: 'add_project' })
    }
  }, [wasm])

  const createRuntimeCb = useAtomCallback(
    useCallback(
      (get, set, wasm: typeof import('@gloo-ai/baml-schema-wasm-web'), envVars: Record<string, string>) => {
        const selectedProject = get(selectedProjectAtom)
        if (!selectedProject) {
          return
        }

        const project_files = get(projectFilesAtom(selectedProject))
        const { project, runtime, diagnostics } = createRuntime(wasm, envVars, selectedProject, project_files)
        set(projectFamilyAtom(selectedProject), project)
        set(runtimeFamilyAtom(selectedProject), {
          last_successful_runtime: undefined,
          current_runtime: runtime,
          diagnostics,
        })
      },
      [wasm, envVars, selectedProject, projectFilesAtom, selectedProjectAtom, projectFamilyAtom, runtimeFamilyAtom],
    ),
  )

  useEffect(() => {
    if (wasm) {
      createRuntimeCb(wasm, envVars)
    }
  }, [wasm, envVars])

  useEffect(() => {
    const fn = (
      event: MessageEvent<
        | {
            command: 'modify_file'
            content: {
              root_path: string
              name: string
              content: string | undefined
            }
          }
        | {
            command: 'add_project'
            content: {
              root_path: string
              files: Record<string, string>
            }
          }
        | {
            command: 'remove_project'
            content: {
              root_path: string
            }
          }
        | {
            command: 'select_function'
            content: {
              root_path: string
              function_name: string
            }
          }
        | {
            command: 'update_cursor'
            content: {
              cursor: { fileName: string; fileText: string; line: number; column: number }
            }
          }
        | {
            command: 'port_number'
            content: {
              port: number
            }
          }
      >,
    ) => {
      const { command, content } = event.data

      switch (command) {
        case 'modify_file':
          updateFile({
            reason: 'modify_file',
            root_path: content.root_path,
            files: [{ name: content.name, content: content.content }],
          })
          break
        case 'add_project':
          updateFile({
            reason: 'add_project',
            root_path: content.root_path,
            files: Object.entries(content.files).map(([name, content]) => ({ name, content })),
            replace_all: true,
          })
          break

        case 'select_function':
          setSelectedFunction(content.function_name)
        case 'update_cursor':
          if ('cursor' in content) {
            updateCursor(content.cursor)
          }
          break

        case 'remove_project':
          removeProject((content as { root_path: string }).root_path)
          break

        case 'port_number':
          console.log('Setting port number', content.port)

          if (content.port === 0) {
            console.error('Port number is 0, cannot launch BAML extension')

            return
          }

          setEnvKeyValueStorage((prev) => {
            let keyExists = false
            const updated: [string, string][] = prev.map(([key, value]) => {
              if (key === 'BOUNDARY_PROXY_URL') {
                keyExists = true
                return [key, `http: //localhost:${content.port}`]
              }
              return [key, value]
            })

            if (!keyExists) {
              updated.push(['BOUNDARY_PROXY_URL', `http://localhost:${content.port}`])
            }
            return updated
          })
          break
      }
    }

    window.addEventListener('message', fn)

    return () => window.removeEventListener('message', fn)
  }, [])

  return (
    <>
      <div className="absolute z-50 flex flex-row gap-2 text-xs bg-transparent right-2 bottom-2">
        <ErrorCount /> <span>Runtime Version: {version}</span>
      </div>
      {selectedProject === null ? (
        availableProjects.length === 0 ? (
          <div>
            No baml projects loaded yet
            <br />
            Open a baml file or wait for the extension to finish loading!
          </div>
        ) : (
          <div>
            <h1>Projects</h1>
            <div>
              {availableProjects.map((root_dir) => (
                <div key={root_dir}>
                  <VSCodeButton onClick={() => setSelectedProject(root_dir)}>{root_dir}</VSCodeButton>
                </div>
              ))}
            </div>
          </div>
        )
      ) : (
        <CustomErrorBoundary>{children}</CustomErrorBoundary>
      )}
    </>
  )
}
