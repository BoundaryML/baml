/// Content once a function has been selected.
import { useAppState } from './AppStateContext'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import React, { useCallback } from 'react'
import {
  ReactFlow,
  addEdge,
  Background,
  useNodesState,
  useEdgesState,
  MiniMap,
  Controls,
  Connection,
} from '@xyflow/react'
import '@xyflow/react/dist/style.css'
import {
  renderPromptAtom,
  selectedFunctionAtom,
  curlAtom,
  streamCurl,
  orchestration_nodes,
  GroupEntry,
  Edge,
} from '../baml_wasm_web/EventListener'
import TestResults from '../baml_wasm_web/test_uis/test_result'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'
import { TooltipProvider } from '../components/ui/tooltip'
import { PromptChunk } from './ImplPanel'
import FunctionTestSnippet from './TestSnippet'
import { Copy } from 'lucide-react'
import { Button } from '../components/ui/button'
import { CheckboxHeader } from './CheckboxHeader'
import { Switch } from '../components/ui/switch'
import CustomErrorBoundary from '../utils/ErrorFallback'
const handleCopy = (text: string) => () => {
  navigator.clipboard.writeText(text)
}

const CurlSnippet: React.FC = () => {
  const rawCurl = useAtomValue(curlAtom) ?? 'Loading...'

  return (
    <div>
      <div className='flex justify-end items-center space-x-2 p-2  rounded-md shadow-sm'>
        <label className='flex items-center space-x-1 mr-2'>
          <Switch
            className='data-[state=checked]:bg-vscode-button-background data-[state=unchecked]:bg-vscode-input-background'
            checked={useAtomValue(streamCurl)}
            onCheckedChange={useSetAtom(streamCurl)}
          />
          <span>View Stream Request</span>
        </label>
        <Button
          onClick={handleCopy(rawCurl)}
          className='py-1 px-3 text-xs text-white bg-vscode-button-background hover:bg-vscode-button-hoverBackground'
        >
          <Copy size={16} />
        </Button>
      </div>
      <PromptChunk
        text={rawCurl}
        client={{
          identifier: {
            end: 0,
            source_file: '',
            start: 0,
            value: 'Curl Request',
          },
          provider: '',
          model: '',
        }}
        showCopy={true}
      />
    </div>
  )
}

interface RenderEdge {
  id: string
  source: string
  target: string
}

interface RenderNode {
  id: string
  type?: string
  data: { label: string }
  position: { x: number; y: number }
  style?: { backgroundColor: string; width?: number; height?: number }
  parentId?: string
  extent?: 'parent' | undefined // Update extent type
}

const ClientGraph: React.FC = () => {
  const graph = useAtomValue(orchestration_nodes)
  const { nodes, edges } = graph
  nodes.forEach((node, index) => {
    console.log(
      `Node ${index}: id: ${node.gid}, letter: ${node.letter}, index: ${node.index}, client_name: ${node.client_name}, parentGid: ${node.parentGid}`,
    )
  })

  edges.forEach((edge, index) => {
    console.log(`Edge ${index}: from ${edge.from_node} to ${edge.to_node}, weight: ${edge.weight}`)
  })

  const renderNodes: RenderNode[] = []
  for (let idx = 0; idx < nodes.length; idx++) {
    const node = nodes[idx]
    renderNodes.push({
      id: node.gid,
      data: { label: node.gid },
      position: { x: 0, y: 0 },
      style: { backgroundColor: 'lightblue', width: 100, height: 100 },
    })
  }

  const renderEdges: RenderEdge[] = edges.map((edge, idx) => ({
    id: idx.toString(),
    source: edge.from_node,
    target: edge.to_node,
  }))

  const [flowNodes, setFlowNodes, onNodesChange] = useNodesState(renderNodes)
  const [flowEdges, setFlowEdges, onEdgesChange] = useEdgesState(renderEdges)

  const onConnect = useCallback((connection: Connection) => {
    setFlowEdges((eds) => addEdge(connection, eds))
  }, [])

  // Synchronize flowNodes and flowEdges with nodes and edges
  React.useEffect(() => {
    setFlowNodes(renderNodes)
    setFlowEdges(renderEdges)
  }, [nodes, edges])

  return (
    <div style={{ height: '100vh', width: '100%' }}>
      <ReactFlow
        nodes={flowNodes}
        edges={flowEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        fitView
      ></ReactFlow>
    </div>
  )
}
const PromptPreview: React.FC = () => {
  const promptPreview = useAtomValue(renderPromptAtom)
  const { showCurlRequest } = useAppState()
  // const { showClientGraph } = useAppState()

  if (!promptPreview) {
    return (
      <div className='flex flex-col items-center justify-center w-full h-full gap-2'>
        <span className='text-center'>No prompt preview available! Add a test to see it!</span>
        <FunctionTestSnippet />
      </div>
    )
  }

  if (typeof promptPreview === 'string') {
    return (
      <PromptChunk
        text={promptPreview}
        type='error'
        client={{
          identifier: {
            end: 0,
            source_file: '',
            start: 0,
            value: 'Error',
          },
          provider: 'baml-openai-chat',
          model: 'gpt-4',
        }}
      />
    )
  }

  if (showCurlRequest) {
    return <ClientGraph />
  }

  return (
    <div className='flex flex-col w-full h-full gap-4 px-2'>
      {promptPreview.as_chat()?.map((chat, idx) => (
        <div key={idx} className='flex flex-col'>
          <div className='flex flex-row'>{chat.role}</div>
          {chat.parts.map((part, idx) => {
            if (part.is_text())
              return (
                <PromptChunk
                  key={idx}
                  text={part.as_text()!}
                  client={{
                    identifier: {
                      end: 0,
                      source_file: '',
                      start: 0,
                      value: promptPreview.client_name,
                    },
                    provider: 'baml-openai-chat',
                    model: 'gpt-4',
                  }}
                />
              )
            if (part.is_image())
              return (
                <a key={idx} href={part.as_image()} target='_blank'>
                  <img key={idx} src={part.as_image()} className='max-w-[400px] object-cover' />
                </a>
              )
            if (part.is_audio()) {
              const audioUrl = part.as_audio()
              if (audioUrl) {
                return (
                  <audio controls key={audioUrl + idx}>
                    <source src={audioUrl} />
                    Your browser does not support the audio element.
                  </audio>
                )
              }
            }
            return null
          })}
        </div>
      ))}
    </div>
  )
}

const FunctionPanel: React.FC = () => {
  const selectedFunc = useAtomValue(selectedFunctionAtom)
  const { showTestResults } = useAppState()

  if (!selectedFunc) {
    const bamlFunctionSnippet = `
function ClassifyConversation(convo: string[]) -> Topic[] {
  client GPT4
  prompt #"
    Classify the CONVERSATION.

    {{ ctx.output_format }}

    CONVERSATION:
    {% for c in convo %}
    {{ c }}
    {% endfor %}
  "#
}

enum Topic {
  TechnicalSupport
  Sales
  CustomerService
  Other
}
  `.trim()
    return (
      <div className='flex flex-col items-center justify-center w-full h-full gap-2'>
        No functions found! You can create a new function like:
        <pre className='p-2 text-xs rounded-sm bg-vscode-input-background'>{bamlFunctionSnippet}</pre>
      </div>
    )
  }

  return (
    <div
      className='flex flex-col w-full overflow-auto'
      style={{
        height: 'calc(100vh - 80px)',
      }}
    >
      <TooltipProvider>
        <ResizablePanelGroup direction='vertical' className='h-full'>
          <ResizablePanel id='top-panel' className='flex w-full px-1' defaultSize={50}>
            <div className='w-full'>
              <ResizablePanelGroup direction='horizontal' className='h-full'>
                <div className='w-full h-full'>
                  <CheckboxHeader />
                  <div className='relative w-full overflow-y-auto' style={{ height: 'calc(100% - 32px)' }}>
                    <PromptPreview />
                  </div>
                </div>
              </ResizablePanelGroup>

              {/* </Allotment> */}
            </div>
          </ResizablePanel>
          {showTestResults && (
            <>
              <ResizableHandle withHandle={false} className='bg-vscode-panel-border' />
              <ResizablePanel
                minSize={10}
                className='flex h-full px-0 py-2 mb-2 border-t border-vscode-textSeparator-foreground'
              >
                <TestResults />
              </ResizablePanel>
            </>
          )}
        </ResizablePanelGroup>
      </TooltipProvider>
    </div>
  )
}

export default FunctionPanel
