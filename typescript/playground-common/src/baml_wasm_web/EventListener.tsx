import 'react18-json-view/src/style.css'
// import * as vscode from 'vscode'

import { VSCodeButton } from '@vscode/webview-ui-toolkit/react'
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai'
import { atomFamily, atomWithStorage, loadable, unwrap, useAtomCallback } from 'jotai/utils'
import { AlertTriangle, CheckCircle, XCircle } from 'lucide-react'
import { useCallback, useEffect } from 'react'
import CustomErrorBoundary from '../utils/ErrorFallback'
import { atomStore, sessionStore, vscodeLocalStorageStore } from './JotaiProvider'
import { availableProjectsAtom, projectFamilyAtom, projectFilesAtom, runtimeFamilyAtom } from './baseAtoms'
import { showClientGraphAtom, showTestsAtom } from './test_uis/testHooks'
import {
  // We _deliberately_ only import types from wasm, instead of importing the module: wasm load is async,
  // so we can only load wasm symbols through wasmAtom, not directly by importing wasm-schema-web
  type WasmDiagnosticError,
  type WasmParam,
  type WasmRuntime,
  type WasmScope,
} from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import { vscode } from '../utils/vscode'
import { useRunHooks } from './test_uis/testHooks'
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

export const wasmAtom = unwrap(wasmAtomAsync)

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
export const bamlCliVersionAtom = atom<string | null>(null)

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
        set(orchIndexAtom, 0)
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
    set(orchIndexAtom, 0)
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

      var selectedFunc = runtime.get_function_at_position(fileName, get(selectedFunctionAtom)?.name ?? '', cursorIdx)

      if (selectedFunc) {
        set(selectedFunctionAtom, selectedFunc.name)
        const selectedTestcase = runtime.get_testcase_from_position(selectedFunc, cursorIdx)

        if (selectedTestcase) {
          set(rawSelectedTestCaseAtom, selectedTestcase.name)
          const nestedFunc = runtime.get_function_of_testcase(fileName, cursorIdx)

          if (nestedFunc) {
            set(selectedFunctionAtom, nestedFunc.name)
          }
        }
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

export const availableClientsAtom = atom<string[]>([])

export const availableFunctionsAtom = atom((get) => {
  const runtime = get(selectedRuntimeAtom)
  if (!runtime) {
    return []
  }
  return runtime.list_functions()
})

export const streamCurlAtom = atom(true)
export const expandImagesAtom = atom(false)

const rawCurlAtomAsync = atom(async (get) => {
  const wasm = get(wasmAtom)
  const runtime = get(selectedRuntimeAtom)
  const func = get(selectedFunctionAtom)
  const test_case = get(selectedTestCaseAtom)
  const orch_index = get(orchIndexAtom)
  if (!wasm || !runtime || !func || !test_case) {
    return null
  }

  const streamCurl = get(streamCurlAtom)
  const expandImages = get(expandImagesAtom)

  const wasmCallContext = new wasm.WasmCallContext()
  wasmCallContext.node_index = orch_index

  return await func.render_raw_curl_for_test(
    runtime,
    test_case.name,
    wasmCallContext,
    streamCurl,
    expandImages,
    async (path: string) => {
      return await vscode.readFile(path)
    },
  )
})

export const rawCurlLoadable = loadable(rawCurlAtomAsync)

const renderPromptAtomAsync = atom(async (get) => {
  const wasm = get(wasmAtom)
  const runtime = get(selectedRuntimeAtom)
  const func = get(selectedFunctionAtom)
  const test_case = get(selectedTestCaseAtom)
  const orch_index = get(orchIndexAtom)
  if (!wasm || !runtime || !func || !test_case) {
    return null
  }

  const wasmCallContext = new wasm.WasmCallContext()
  wasmCallContext.node_index = orch_index

  try {
    return await func.render_prompt_for_test(runtime, test_case.name, wasmCallContext)
  } catch (e) {
    if (e instanceof Error) {
      return e.message
    } else {
      return `${e}`
    }
  }
})

export const renderPromptAtom = unwrap(renderPromptAtomAsync)

export interface TypeCount {
  // options are F (Fallback), R (Retry), D (Direct), B (Round Robin)
  type: string

  // range from 0 to n
  index: number
  scope_name: string

  //only for retry
  retry_delay?: number
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
  node_index?: number
}
export interface GroupEntry {
  letter: string
  index: number
  orch_index?: number
  client_name?: string
  gid: ReturnType<typeof uuid>
  parentGid?: ReturnType<typeof uuid>
  Position?: Position
  Dimension?: Dimension
}

export interface Dimension {
  width: number
  height: number
}

export const orchIndexAtom = atom(0)
export const currentClientsAtom = atom((get) => {
  return []
})
export const orchestration_nodes = atom((get): { nodes: GroupEntry[]; edges: Edge[] } => {
  return { nodes: [], edges: [] }
})
// export const currentClientsAtom = atom((get) => {
//   const func = get(selectedFunctionAtom)
//   const runtime = get(selectedRuntimeAtom)
//   if (!func || !runtime) {
//     return []
//   }

//   const wasmScopes = func.orchestration_graph(runtime)
//   if (wasmScopes === null) {
//     return []
//   }

//   const nodes = createClientNodes(wasmScopes)
//   return nodes.map((node) => node.name)
// })
// // something about the orchestration graph is broken, comment it out to make it work
// export const orchestration_nodes = atom((get): { nodes: GroupEntry[]; edges: Edge[] } => {
//   const func = get(selectedFunctionAtom)
//   const runtime = get(selectedRuntimeAtom)
//   if (!func || !runtime) {
//     return { nodes: [], edges: [] }
//   }

//   const wasmScopes = func.orchestration_graph(runtime)
//   if (wasmScopes === null) {
//     return { nodes: [], edges: [] }
//   }

//   const nodes = createClientNodes(wasmScopes)
//   const { unitNodes, groups } = buildUnitNodesAndGroups(nodes)

//   const edges = createEdges(unitNodes)

//   const positionedNodes = getPositions(groups)

//   positionedNodes.forEach((posNode) => {
//     const correspondingUnitNode = unitNodes.find((unitNode) => unitNode.gid === posNode.gid)
//     if (correspondingUnitNode) {
//       posNode.orch_index = correspondingUnitNode.node_index
//     }
//   })

//   return { nodes: positionedNodes, edges }
// })

interface Position {
  x: number
  y: number
}

function getPositions(nodes: { [key: string]: GroupEntry }): GroupEntry[] {
  const nodeEntries = Object.values(nodes)
  if (nodeEntries.length === 0) {
    return []
  }

  const adjacencyList: { [key: string]: string[] } = {}

  nodeEntries.forEach((node) => {
    if (node.parentGid) {
      if (!adjacencyList[node.parentGid]) {
        adjacencyList[node.parentGid] = []
      }
      adjacencyList[node.parentGid].push(node.gid)
    }
    if (!adjacencyList[node.gid]) {
      adjacencyList[node.gid] = []
    }
  })

  const rootNode = nodeEntries.find((node) => !node.parentGid)
  if (!rootNode) {
    console.error('No root node found')
    return []
  }

  const sizes = getSizes(adjacencyList, rootNode.gid)

  const positionsMap = getCoordinates(adjacencyList, rootNode.gid, sizes)
  const positionedNodes = nodeEntries.map((node) => ({
    ...node,
    Position: positionsMap[node.gid] || { x: 0, y: 0 },
    Dimension: sizes[node.gid] || { width: 0, height: 0 },
  }))

  return positionedNodes
}

function getCoordinates(
  adjacencyList: { [key: string]: string[] },
  rootNode: string,
  sizes: { [key: string]: { width: number; height: number } },
): { [key: string]: Position } {
  if (Object.keys(adjacencyList).length === 0 || Object.keys(sizes).length === 0) {
    return {}
  }

  const coordinates: { [key: string]: Position } = {}

  const PADDING = 60 // Define a constant padding value

  function recurse(node: string, horizontal: boolean, x: number, y: number): { x: number; y: number } {
    const children = adjacencyList[node]
    if (children.length === 0) {
      coordinates[node] = { x, y }
      return coordinates[node]
    }

    let childX = PADDING
    let childY = PADDING
    for (const child of children) {
      const childSize = recurse(child, !horizontal, childX, childY)

      if (!horizontal) {
        childY = childSize.y + PADDING + sizes[child].height
      } else {
        childX = childSize.x + PADDING + sizes[child].width
      }
    }

    coordinates[node] = { x, y }
    return coordinates[node]
  }

  recurse(rootNode, true, 0, 0)
  return coordinates
}

function getSizes(
  adjacencyList: { [key: string]: string[] },
  rootNode: string,
): { [key: string]: { width: number; height: number } } {
  if (Object.keys(adjacencyList).length === 0) {
    return {}
  }

  const sizes: { [key: string]: { width: number; height: number } } = {}

  const PADDING = 60 // Define a constant padding value

  function recurse(node: string, horizontal: boolean): { width: number; height: number } {
    const children = adjacencyList[node]
    if (children.length === 0) {
      sizes[node] = { width: 100, height: 50 }
      return sizes[node]
    }

    let width = horizontal ? PADDING : 0
    let height = horizontal ? 0 : PADDING
    for (const child of children) {
      const childSize = recurse(child, !horizontal)

      if (!horizontal) {
        width = Math.max(width, childSize.width)
        height += childSize.height + PADDING
      } else {
        width += childSize.width + PADDING
        height = Math.max(height, childSize.height)
      }
    }

    if (!horizontal) {
      width += 2 * PADDING // Add padding to the final width
    } else {
      height += 2 * PADDING // Add padding to the final height
    }

    sizes[node] = { width, height }
    return sizes[node]
  }

  recurse(rootNode, true)

  return sizes
}

function createClientNodes(wasmScopes: any[]): ClientNode[] {
  let indexOuter = 0
  const nodes: ClientNode[] = []

  for (const scope of wasmScopes) {
    const scopeInfo = scope.get_orchestration_scope_info()
    const scopePath = scopeInfo as any[]

    const stackGroup = createStackGroup(scopePath)

    // Always a direct node
    const lastScope = scopePath[scopePath.length - 1]

    const clientNode: ClientNode = {
      name: lastScope.name,
      node_index: indexOuter,
      type: lastScope.type,
      identifier: stackGroup,
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
    const indexVal = scope.type === 'Retry' ? scope.count : scope.type === 'Direct' ? 0 : scope.index

    stackGroup.push({
      type: getTypeLetter(scope.type),
      index: indexVal,
      scope_name: scope.type === 'RoundRobin' ? scope.strategy_name : scope.name ?? 'SOME_NAME',
    })

    if (scope.type === 'Retry') {
      stackGroup[stackGroup.length - 1].retry_delay = scope.delay
    }
  }

  return stackGroup
}

function buildUnitNodesAndGroups(nodes: ClientNode[]): {
  unitNodes: NodeEntry[]
  groups: { [gid: string]: GroupEntry }
} {
  const unitNodes: NodeEntry[] = []
  const groups: { [gid: string]: GroupEntry } = {}
  const prevNodeIndexGroups: GroupEntry[] = []

  for (let index = 0; index < nodes.length; index++) {
    const node = nodes[index]
    const stackGroup = node.identifier
    let parentGid = ''
    let retry_cost = -1
    for (let stackIndex = 0; stackIndex < stackGroup.length; stackIndex++) {
      const scopeLayer = stackGroup[stackIndex]
      const prevScopeIdx = stackIndex > 0 ? stackGroup[stackIndex - 1].index : 0
      const prevNodeScope = prevNodeIndexGroups.at(stackIndex)
      const curGid = getScopeDetails(scopeLayer, prevScopeIdx, prevNodeScope)

      if (!(curGid in groups)) {
        groups[curGid] = {
          letter: scopeLayer.type,
          index: prevScopeIdx,
          client_name: scopeLayer.scope_name,
          gid: curGid,
          ...(parentGid && { parentGid }),
        }
        // Also clean indexGroups up to the current stackIndex
        prevNodeIndexGroups.length = stackIndex
      }

      prevNodeIndexGroups[stackIndex] = {
        letter: scopeLayer.type,
        index: prevScopeIdx,
        client_name: scopeLayer.scope_name,
        gid: curGid,
        ...(parentGid && { parentGid }),
      }

      parentGid = curGid

      if (scopeLayer.type === 'R' && scopeLayer.retry_delay !== 0) {
        retry_cost = scopeLayer.retry_delay ?? -1
      }
    }

    unitNodes.push({
      gid: parentGid,
      node_index: index,
      ...(retry_cost !== -1 && { weight: retry_cost }),
    })
  }

  return { unitNodes, groups }
}
var counter = 0
function uuid() {
  return String(counter++)
}
function getScopeDetails(scopeLayer: TypeCount, prevIdx: number, prevIndexGroupEntry: GroupEntry | undefined) {
  if (prevIndexGroupEntry === undefined) {
    return uuid()
  } else {
    const indexEntryGid = prevIndexGroupEntry.gid
    const indexEntryIdx = prevIndexGroupEntry.index
    const indexEntryScopeName = prevIndexGroupEntry.client_name

    switch (scopeLayer.type) {
      case 'B':
        if (scopeLayer.scope_name === indexEntryScopeName) {
          return indexEntryGid
        } else {
          return uuid()
        }
      default:
        if (prevIdx === indexEntryIdx) {
          return indexEntryGid
        } else {
          return uuid()
        }
    }
  }
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
      <div className='flex flex-row items-center gap-1 text-green-600'>
        <CheckCircle size={12} />
      </div>
    )
  }
  if (errors === 0) {
    return (
      <div className='flex flex-row items-center gap-1 text-yellow-600'>
        {warnings} <AlertTriangle size={12} />
      </div>
    )
  }
  return (
    <div className='flex flex-row items-center gap-1 text-red-600'>
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
  const [bamlCliVersion, setBamlCliVersion] = useAtom(bamlCliVersionAtom)
  const { isRunning, run } = useRunHooks()
  const setShowTests = useSetAtom(showTestsAtom)
  const setClientGraph = useSetAtom(showClientGraphAtom)
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
        | {
            command: 'baml_cli_version'
            content: string
          }
        | {
            command: 'run_test'
            content: { test_name: string }
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
          if (content && content.root_path) {
            updateFile({
              reason: 'add_project',
              root_path: content.root_path,
              files: Object.entries(content.files).map(([name, content]) => ({ name, content })),
              replace_all: true,
            })
          }
          break

        case 'select_function':
          setSelectedFunction(content.function_name)
        case 'update_cursor':
          if ('cursor' in content) {
            updateCursor(content.cursor)
          }
          break
        case 'baml_cli_version':
          setBamlCliVersion(content)
          break

        case 'remove_project':
          removeProject((content as { root_path: string }).root_path)
          break

        case 'port_number':
          if (content.port === 0) {
            console.error('No ports available, cannot launch BAML extension')

            return
          }

          setEnvKeyValueStorage((prev) => {
            let keyExists = false
            const updated: [string, string][] = prev.map(([key, value]) => {
              if (key === 'BOUNDARY_PROXY_URL') {
                keyExists = true
                return [key, `http://localhost:${content.port}`]
              }
              return [key, value]
            })

            if (!keyExists) {
              updated.push(['BOUNDARY_PROXY_URL', `http://localhost:${content.port}`])
            }
            return updated
          })
          break

        case 'run_test':
          run([content.test_name])
          setShowTests(true)
          setClientGraph(false)
          break
      }
    }

    window.addEventListener('message', fn)

    return () => window.removeEventListener('message', fn)
  }, [])

  return (
    <>
      <div className='absolute z-50 flex flex-row gap-2 text-xs bg-transparent right-2 bottom-2'>
        <div className='pr-4 whitespace-nowrap'>{bamlCliVersion && 'baml-cli ' + bamlCliVersion}</div>
        <ErrorCount /> <span>VSCode Runtime Version: {version}</span>
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
