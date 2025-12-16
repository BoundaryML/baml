import { useAtom } from 'jotai'
import {
  ReactFlow,
  addEdge,
  Background,
  useNodesState,
  useEdgesState,
  MiniMap,
  MarkerType,
  Connection,
} from '@xyflow/react'
import { useAtomValue } from 'jotai'
import type React from 'react'
import { useMemo } from 'react'
import { currentClientsAtom, orchestrationNodesAtom, orchIndexAtom } from '../../../atoms-orch-graph'
import { useEffect } from 'react'
import '@xyflow/react/dist/style.css'

interface RenderEdge {
  id: string
  source: string
  target: string
}

interface RenderNode {
  id: string
  data: { label: string; orch_index: number }
  position: { x: number; y: number }
  style?: { backgroundColor: string; width?: number; height?: number }
  parentId?: string
  extent?: 'parent' | undefined // Update extent type
}

const ClientHeader: React.FC = () => {
  const orchIndex = useAtomValue(orchIndexAtom)

  const clientsArray = useAtomValue(currentClientsAtom)
  const currentClient = clientsArray[orchIndex]
  return (
    <div className='pt-4'>
      <div className='flex flex-col-reverse items-start gap-0.5'>
        <span className='pl-2 text-xs text-muted-foreground flex flex-row flex-wrap items-center gap-0.5'>
          {clientsArray.length > 1 && `Attempt ${orchIndex} in Client Graph`}
        </span>
        <div className='max-w-[300px] justify-start items-center flex hover:bg-vscode-button-hoverBackground h-fit rounded-md text-vscode-foreground cursor-pointer'>
          <span className='px-2 py-1 w-full text-left truncate'>{currentClient}</span>
        </div>
      </div>
    </div>
  )
}

export const ClientGraphView: React.FC = () => {
  const graph = useAtomValue(orchestrationNodesAtom)
  const [orchIndex, setOrchIndex] = useAtom(orchIndexAtom)

  const renderNodes: RenderNode[] = useMemo(
    () =>
      graph.nodes.map((node) => ({
        id: node.gid,
        data: {
          label: node.client_name ?? 'no name for this node',
          orch_index: node.orch_index !== undefined ? node.orch_index : -1,
        },
        position: { x: node.Position?.x ?? 0, y: node.Position?.y ?? 0 },
        style: {
          backgroundColor: 'rgba(255, 0, 255, 0.2)',
          width: node.Dimension?.width,
          height: node.Dimension?.height,
          outline: orchIndex === node.orch_index ? '1px solid white' : '',
        },
        parentId: node.parentGid,
        extent: 'parent',
      })),
    [graph.nodes, orchIndex],
  )

  const renderEdges: RenderEdge[] = useMemo(
    () =>
      graph.edges.map((edge, idx) => ({
        id: idx.toString(),
        source: edge.from_node,
        target: edge.to_node,
        animated: true,
        type: 'smoothstep',
        markerEnd: {
          type: MarkerType.ArrowClosed,
        },
        label: edge.weight !== undefined ? `⏰ ${edge.weight} ms ` : '',
      })),
    [graph.edges],
  )

  const [flowNodes, setFlowNodes, onNodesChange] = useNodesState(renderNodes)
  const [flowEdges, setFlowEdges, onEdgesChange] = useEdgesState(renderEdges)

  // const onConnect = useCallback((connection: Connection) => {
  //   setFlowEdges((eds) => addEdge(connection, eds))
  // }, [])

  // Set default selected node

  // Synchronize flowNodes and flowEdges with nodes and edges
  useEffect(() => {
    setFlowNodes(renderNodes)
    setFlowEdges(renderEdges)
  }, [renderNodes, renderEdges])

  const onNodeClick = (event: React.MouseEvent, node: any) => {
    if (node.data.orch_index != -1) {
      setOrchIndex(node.data.orch_index)
    }
  }

  const styles = {
    whiteSpace: 'normal',
    wordWrap: 'break-word',
    overflowWrap: 'break-word',
  } as const

  return (
    <div className='flex flex-col w-full h-[400px]'>
      <ClientHeader />
      <div className='h-full'>
        <ReactFlow
          style={styles}
          nodes={flowNodes}
          edges={flowEdges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeClick={onNodeClick}
          fitView
          edgesFocusable={false}
          nodesDraggable={false}
          nodesConnectable={false}
          nodesFocusable={false}
        />
      </div>
    </div>
  )
}
