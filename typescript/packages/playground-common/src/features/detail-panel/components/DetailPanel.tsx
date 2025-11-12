/**
 * DetailPanel Component
 *
 * Shows detailed information about a selected node
 * Tabs: I/O, Logs, History
 */

import {
  Terminal,
  MousePointerClick,
  Play,
  FileInput,
  FileOutput,
  ScrollText,
  ChevronDown,
  CheckCircle2,
  XCircle,
  Clock,
  RefreshCw,
  Sparkles,
  MessageSquare,
  Database,
  type LucideIcon
} from 'lucide-react';
import { useActiveNode, useDetailPanel, useNodeInputSources, useSelectedInputSource } from '../../../sdk/hooks';
import { useState, useRef, useEffect, useMemo } from 'react';
import { useAtomValue } from 'jotai';
import { selectedTestCaseNameAtom } from '../../../sdk/atoms/core.atoms';
import type { GraphNode, NodeExecution, InputSource } from '../../../sdk/types';
import { useBAMLSDK } from '../../../sdk';

// Tab Component Props
interface IOTabProps {
  node: GraphNode;
  execution: NodeExecution | null | undefined;
}

// Reusable Empty State Component
function EmptyState({
  icon: Icon,
  title,
  description,
  action
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="flex flex-col items-center justify-center h-full py-12 px-4 text-center">
      <div className="rounded-full bg-muted/50 p-4 mb-4">
        <Icon className="w-8 h-8 text-muted-foreground" />
      </div>
      <h3 className="text-sm font-semibold mb-2">{title}</h3>
      <p className="text-xs text-muted-foreground max-w-sm mb-4">{description}</p>
      {action && <div>{action}</div>}
    </div>
  );
}

export function DetailPanel() {
  const activeNode = useActiveNode();
  const { isOpen, close } = useDetailPanel();

  if (!isOpen) {
    return null;
  }

  // Show "no node selected" state
  if (!activeNode) {
    return (
      <div className="h-full flex flex-col bg-card border-t border-border">
        <div className="flex items-center justify-between px-2 py-1 border-b border-border">
          <h3 className="text-xs font-semibold">Node Details</h3>
          <button
            onClick={close}
            className="p-0.5 hover:bg-muted rounded transition-colors"
            aria-label="Close detail panel"
          >
            <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <EmptyState
          icon={MousePointerClick}
          title="No Node Selected"
          description="Click on any node in the graph to view its details, execution status, inputs/outputs, and logs."
        />
      </div>
    );
  }

  const { node, execution, state } = activeNode;

  return (
    <div className="h-full flex flex-col bg-card border-t border-border">
      {/* Compact Header - single row */}
      <div className="flex items-center justify-between px-2 py-1 border-b border-border">
        <div className="flex items-center gap-1.5">
          {node.type === 'llm_function' && (
            <span className="px-1 py-0.5 rounded text-[9px] font-bold bg-purple-500 text-white">
              LLM
            </span>
          )}
          <h3 className={`text-xs font-semibold truncate max-w-[300px] ${node.type === 'llm_function' ? 'text-purple-600 dark:text-purple-400' : ''}`}>
            {node.label}
          </h3>
          {state && (
            <span
              className={`text-[9px] px-1 py-0.5 rounded ${state === 'running'
                ? 'bg-blue-100 text-blue-700 dark:bg-blue-900 dark:text-blue-300'
                : state === 'success'
                  ? 'bg-green-100 text-green-700 dark:bg-green-900 dark:text-green-300'
                  : state === 'error'
                    ? 'bg-red-100 text-red-700 dark:bg-red-900 dark:text-red-300'
                    : 'bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-300'
                }`}
            >
              {state}
            </span>
          )}
        </div>

        {/* Close button */}
        <button
          onClick={close}
          className="p-0.5 hover:bg-muted rounded transition-colors"
          aria-label="Close detail panel"
        >
          <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      {/* Content - no tabs, directly show content */}
      <div className="p-2 flex-1 overflow-auto text-xs">
        {node.type === 'llm_function' ? (
          <LLMNodeContent node={node} execution={execution} />
        ) : (
          <StandardNodeContent node={node} execution={execution} />
        )}
      </div>
    </div>
  );
}

// LLM Node Content - Shows inputs, outputs, client, request, response
function LLMNodeContent({ node, execution }: IOTabProps) {
  const sdk = useBAMLSDK();
  const executionInputSources = useNodeInputSources(node.id);
  const { selectedSource, selectSource } = useSelectedInputSource();
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const testName = useAtomValue(selectedTestCaseNameAtom);

  // Fetch test cases and merge with execution inputs
  const allInputSources = useMemo(() => {
    const testCases = sdk.testCases.get(node.functionName ?? node.id);
    return [...testCases, ...executionInputSources] as InputSource[];
  }, [sdk, node.functionName, node.id, executionInputSources]);

  // If a test case is selected in unified state, prefer that source
  useEffect(() => {
    if (!testName) return;
    if (selectedSource?.nodeId === node.id && selectedSource.sourceType === 'test') {
      const current = allInputSources.find((s) => s.id === selectedSource.sourceId);
      if (current && (current.name === testName || current.id === testName || current.id.endsWith(`_${testName}`))) {
        return;
      }
    }

    const matchingSource = allInputSources.find(
      (source) =>
        source.source === 'test' &&
        (source.name === testName || source.id === testName || source.id.endsWith(`_${testName}`))
    );

    if (matchingSource) {
      selectSource(node.id, 'test', matchingSource.id);
    }
  }, [testName, allInputSources, node.id, selectSource, selectedSource]);

  // Auto-select the latest available input source (only if no test is explicitly selected)
  useEffect(() => {
    // Don't auto-select if a source is already selected for this node
    if (selectedSource?.nodeId === node.id) return;

    // Don't auto-select if a test case is explicitly selected (give testName effect priority)
    if (testName) return;

    let latestSource: InputSource | null = null;
    let latestTimestamp = 0;

    allInputSources.forEach((source) => {
      let sourceTimestamp = 0;
      if (source.source === 'execution') {
        sourceTimestamp = source.timestamp;
      } else if (source.source === 'test' && source.lastRun) {
        sourceTimestamp = source.lastRun;
      }

      if (sourceTimestamp > latestTimestamp) {
        latestTimestamp = sourceTimestamp;
        latestSource = source;
      }
    });

    if (execution?.executionId) {
      const execSource = executionInputSources.find(s => s.source === 'execution' && s.executionId === execution.executionId);
      const execTimestamp = (execSource && execSource.source === 'execution') ? execSource.timestamp : 0;
      if (execTimestamp > latestTimestamp) {
        selectSource(node.id, 'execution', execution.executionId);
        return;
      }
    }

    if (latestSource !== null) {
      const source = latestSource as InputSource;
      selectSource(node.id, source.source, source.id);
    }
  }, [allInputSources, execution?.executionId, executionInputSources, node.id, selectedSource?.nodeId, selectSource, testName]);

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!isDropdownOpen) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsDropdownOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isDropdownOpen]);

  const currentSource = selectedSource?.nodeId === node.id
    ? allInputSources.find((s) => s.id === selectedSource.sourceId)
    : null;

  const displayedInputs = currentSource?.inputs || execution?.inputs || {};
  const displayedOutputs = currentSource && currentSource.source === 'execution'
    ? currentSource.outputs
    : execution?.outputs;

  // Mock LLM metadata (in real implementation, this would come from execution data)
  const llmClient = execution?.metadata?.llmClient || 'gpt-4';
  const llmRequest = execution?.metadata?.llmRequest || displayedInputs;
  const llmResponse = execution?.metadata?.llmResponse || displayedOutputs;

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'success':
        return <CheckCircle2 className="w-3 h-3 text-green-600 dark:text-green-400" />;
      case 'error':
        return <XCircle className="w-3 h-3 text-red-600 dark:text-red-400" />;
      case 'running':
        return <Clock className="w-3 h-3 text-blue-600 dark:text-blue-400" />;
      default:
        return null;
    }
  };

  if (!execution && allInputSources.length === 0) {
    return (
      <EmptyState
        icon={Sparkles}
        title="No Execution Data"
        description="Run the workflow or add test cases to see LLM interactions."
      />
    );
  }

  return (
    <div className="space-y-2">
      {/* Compact Input Source Row */}
      <div className="flex items-center gap-1 text-xs">
        <span className="text-muted-foreground shrink-0">Input</span>

        {allInputSources.length > 0 && (
          <div className="relative flex-1" ref={dropdownRef}>
            <button
              onClick={() => setIsDropdownOpen(!isDropdownOpen)}
              className="w-full flex items-center justify-between gap-1 px-1.5 py-0.5 text-xs bg-muted/30 hover:bg-muted/50 border border-muted rounded transition-colors"
            >
              <div className="flex items-center gap-1 truncate">
                {currentSource ? (
                  <>
                    <span className="truncate">{currentSource.name}</span>
                    {currentSource.source === 'test' && currentSource.status && getStatusIcon(currentSource.status)}
                    {currentSource.source === 'execution' && getStatusIcon(currentSource.status)}
                  </>
                ) : execution ? (
                  <>
                    <span>Latest</span>
                    {getStatusIcon(execution.state === 'success' ? 'success' : execution.state === 'error' ? 'error' : 'running')}
                  </>
                ) : (
                  <span className="text-muted-foreground italic">None</span>
                )}
              </div>
              <ChevronDown className={`w-3 h-3 shrink-0 transition-transform ${isDropdownOpen ? 'rotate-180' : ''}`} />
            </button>

            {isDropdownOpen && (
              <div className="absolute z-10 w-full mt-1 bg-popover border border-border rounded-md shadow-lg max-h-48 overflow-auto">
                {execution && (
                  <button
                    onClick={() => {
                      selectSource(node.id, 'execution', execution.executionId);
                      setIsDropdownOpen(false);
                    }}
                    className="w-full px-2 py-1 text-xs text-left hover:bg-muted/50 flex items-center justify-between"
                  >
                    <span>Latest Execution</span>
                    {getStatusIcon(execution.state === 'success' ? 'success' : execution.state === 'error' ? 'error' : 'running')}
                  </button>
                )}
                {allInputSources.filter(s => s.source === 'test').length > 0 && (
                  <>
                    <div className="px-2 py-0.5 text-[10px] font-semibold text-muted-foreground border-t">TEST CASES</div>
                    {allInputSources.filter(s => s.source === 'test').map((source) => (
                      <button
                        key={source.id}
                        onClick={() => {
                          selectSource(node.id, source.source, source.id);
                          setIsDropdownOpen(false);
                        }}
                        className="w-full px-2 py-1 text-xs text-left hover:bg-muted/50 flex items-center justify-between"
                      >
                        <span>{source.name}</span>
                        {source.source === 'test' && source.status && getStatusIcon(source.status)}
                      </button>
                    ))}
                  </>
                )}
                {executionInputSources.length > 0 && (
                  <>
                    <div className="px-2 py-0.5 text-[10px] font-semibold text-muted-foreground border-t">PREVIOUS EXECUTIONS</div>
                    {executionInputSources.map((source) => (
                      <button
                        key={source.id}
                        onClick={() => {
                          selectSource(node.id, source.source, source.id);
                          setIsDropdownOpen(false);
                        }}
                        className="w-full px-2 py-1 text-xs text-left hover:bg-muted/50 flex items-center justify-between"
                      >
                        <div className="flex items-center gap-1.5">
                          <span>{source.name}</span>
                          {source.source === 'execution' && (
                            <span className="text-[10px] text-muted-foreground">
                              ({new Date(source.timestamp).toLocaleTimeString()})
                            </span>
                          )}
                        </div>
                        {source.source === 'execution' && getStatusIcon(source.status)}
                      </button>
                    ))}
                  </>
                )}
              </div>
            )}
          </div>
        )}

        {/* Compact Action Buttons */}
        <button
          className="px-1.5 py-0.5 text-xs font-medium bg-blue-600 hover:bg-blue-700 disabled:bg-blue-600/50 text-white rounded flex items-center gap-0.5 shrink-0"
          disabled={!currentSource}
          onClick={async () => {
            if (!currentSource) return;
            const activeWorkflow = sdk.workflows.getActive();
            if (!activeWorkflow) return;
            await sdk.executions.start(activeWorkflow.id, currentSource.inputs, { startFromNodeId: node.id });
          }}
        >
          <Play className="w-2.5 h-2.5" />
          Run from here
        </button>
        <button
          className="px-1.5 py-0.5 text-xs font-medium bg-purple-600 hover:bg-purple-700 disabled:bg-purple-600/50 text-white rounded flex items-center gap-0.5 shrink-0"
          disabled={!currentSource}
          title="Replay this node only"
          onClick={async () => {
            if (!currentSource) return;
            // TODO: Implement single-node replay
            alert('Replay node - Coming soon!\n\nThis will re-execute just this single node with the selected inputs.');
          }}
        >
          <RefreshCw className="w-2.5 h-2.5" />
          Replay
        </button>
      </div>

      {/* LLM Client Info */}
      <div className="flex items-center gap-1.5 py-1">
        <Database className="w-3 h-3 text-purple-600 dark:text-purple-400" />
        <span className="text-xs font-medium">Client:</span>
        <span className="text-xs text-purple-600 dark:text-purple-400 font-mono">{llmClient}</span>
      </div>

      {/* Input/Output Grid */}
      <div className="grid grid-cols-2 gap-2">
        {/* Input */}
        <div>
          <div className="flex items-center gap-1 mb-1">
            <FileInput className="w-3 h-3 text-muted-foreground" />
            <span className="text-[11px] font-semibold text-muted-foreground">Input</span>
          </div>
          <pre className="bg-muted p-1.5 rounded text-[10px] overflow-auto max-h-32 font-mono">
            {JSON.stringify(displayedInputs, null, 2)}
          </pre>
        </div>

        {/* Output */}
        <div>
          <div className="flex items-center gap-1 mb-1">
            <FileOutput className="w-3 h-3 text-muted-foreground" />
            <span className="text-[11px] font-semibold text-muted-foreground">Output</span>
          </div>
          {displayedOutputs ? (
            <pre className="bg-muted p-1.5 rounded text-[10px] overflow-auto max-h-32 font-mono">
              {JSON.stringify(displayedOutputs, null, 2)}
            </pre>
          ) : (
            <div className="bg-muted/30 border border-dashed border-muted rounded p-1.5 text-[10px] text-muted-foreground italic">
              {execution?.state === 'running' ? 'Waiting...' : 'No output'}
            </div>
          )}
        </div>
      </div>

      {/* LLM Request */}
      <div>
        <div className="flex items-center gap-1 mb-1">
          <MessageSquare className="w-3 h-3 text-blue-600 dark:text-blue-400" />
          <span className="text-[11px] font-semibold text-muted-foreground">LLM Request</span>
        </div>
        <pre className="bg-muted p-1.5 rounded text-[10px] overflow-auto max-h-32 font-mono">
          {JSON.stringify(llmRequest, null, 2)}
        </pre>
      </div>

      {/* LLM Response */}
      <div>
        <div className="flex items-center gap-1 mb-1">
          <Sparkles className="w-3 h-3 text-purple-600 dark:text-purple-400" />
          <span className="text-[11px] font-semibold text-muted-foreground">LLM Response</span>
        </div>
        {llmResponse ? (
          <pre className="bg-muted p-1.5 rounded text-[10px] overflow-auto max-h-32 font-mono">
            {JSON.stringify(llmResponse, null, 2)}
          </pre>
        ) : (
          <div className="bg-muted/30 border border-dashed border-muted rounded p-1.5 text-[10px] text-muted-foreground italic">
            {execution?.state === 'running' ? 'Waiting for response...' : 'No response'}
          </div>
        )}
      </div>
    </div>
  );
}

// Standard Node Content - Shows inputs, outputs, and logs with test case support
function StandardNodeContent({ node, execution }: IOTabProps) {
  const sdk = useBAMLSDK();
  const executionInputSources = useNodeInputSources(node.id);
  const { selectedSource, selectSource } = useSelectedInputSource();
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);
  const testName = useAtomValue(selectedTestCaseNameAtom);
  const logs = execution?.logs || [];

  // Fetch test cases and merge with execution inputs
  const allInputSources = useMemo(() => {
    const testCases = sdk.testCases.get(node.functionName ?? node.id);
    return [...testCases, ...executionInputSources] as InputSource[];
  }, [sdk, node.functionName, node.id, executionInputSources]);

  // Auto-select the latest available input source (only if no test is explicitly selected)
  useEffect(() => {
    // Don't auto-select if a source is already selected for this node
    if (selectedSource?.nodeId === node.id) return;

    // Don't auto-select if a test case is explicitly selected (give testName effect priority)
    if (testName) return;

    let latestSource: InputSource | null = null;
    let latestTimestamp = 0;

    allInputSources.forEach((source) => {
      let sourceTimestamp = 0;
      if (source.source === 'execution') {
        sourceTimestamp = source.timestamp;
      } else if (source.source === 'test' && source.lastRun) {
        sourceTimestamp = source.lastRun;
      }

      if (sourceTimestamp > latestTimestamp) {
        latestTimestamp = sourceTimestamp;
        latestSource = source;
      }
    });

    if (execution?.executionId) {
      const execSource = executionInputSources.find(s => s.source === 'execution' && s.executionId === execution.executionId);
      const execTimestamp = (execSource && execSource.source === 'execution') ? execSource.timestamp : 0;
      if (execTimestamp > latestTimestamp) {
        selectSource(node.id, 'execution', execution.executionId);
        return;
      }
    }

    if (latestSource !== null) {
      const source = latestSource as InputSource;
      selectSource(node.id, source.source, source.id);
    }
  }, [allInputSources, execution?.executionId, executionInputSources, node.id, selectedSource?.nodeId, selectSource, testName]);

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!isDropdownOpen) return;
    const handleClickOutside = (event: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsDropdownOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [isDropdownOpen]);

  const currentSource = selectedSource?.nodeId === node.id
    ? allInputSources.find((s) => s.id === selectedSource.sourceId)
    : null;

  const displayedInputs = currentSource?.inputs || execution?.inputs || {};
  const displayedOutputs = currentSource && currentSource.source === 'execution'
    ? currentSource.outputs
    : execution?.outputs;

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'success':
        return <CheckCircle2 className="w-3 h-3 text-green-600 dark:text-green-400" />;
      case 'error':
        return <XCircle className="w-3 h-3 text-red-600 dark:text-red-400" />;
      case 'running':
        return <Clock className="w-3 h-3 text-blue-600 dark:text-blue-400" />;
      default:
        return null;
    }
  };

  if (!execution && allInputSources.length === 0) {
    return (
      <EmptyState
        icon={Terminal}
        title="No Execution Data"
        description="Run the workflow or add test cases to see data from this node."
      />
    );
  }

  return (
    <div className="space-y-2">
      {/* Compact Input Source Row */}
      <div className="flex items-center gap-1 text-xs">
        <span className="text-muted-foreground shrink-0">Input</span>

        {allInputSources.length > 0 && (
          <div className="relative flex-1" ref={dropdownRef}>
            <button
              onClick={() => setIsDropdownOpen(!isDropdownOpen)}
              className="w-full flex items-center justify-between gap-1 px-1.5 py-0.5 text-xs bg-muted/30 hover:bg-muted/50 border border-muted rounded transition-colors"
            >
              <div className="flex items-center gap-1 truncate">
                {currentSource ? (
                  <>
                    <span className="truncate">{currentSource.name}</span>
                    {currentSource.source === 'test' && currentSource.status && getStatusIcon(currentSource.status)}
                    {currentSource.source === 'execution' && getStatusIcon(currentSource.status)}
                  </>
                ) : execution ? (
                  <>
                    <span>Latest</span>
                    {getStatusIcon(execution.state === 'success' ? 'success' : execution.state === 'error' ? 'error' : 'running')}
                  </>
                ) : (
                  <span className="text-muted-foreground italic">None</span>
                )}
              </div>
              <ChevronDown className={`w-3 h-3 shrink-0 transition-transform ${isDropdownOpen ? 'rotate-180' : ''}`} />
            </button>

            {isDropdownOpen && (
              <div className="absolute z-10 w-full mt-1 bg-popover border border-border rounded-md shadow-lg max-h-48 overflow-auto">
                {execution && (
                  <button
                    onClick={() => {
                      selectSource(node.id, 'execution', execution.executionId);
                      setIsDropdownOpen(false);
                    }}
                    className="w-full px-2 py-1 text-xs text-left hover:bg-muted/50 flex items-center justify-between"
                  >
                    <span>Latest Execution</span>
                    {getStatusIcon(execution.state === 'success' ? 'success' : execution.state === 'error' ? 'error' : 'running')}
                  </button>
                )}
                {allInputSources.filter(s => s.source === 'test').length > 0 && (
                  <>
                    <div className="px-2 py-0.5 text-[10px] font-semibold text-muted-foreground border-t">TEST CASES</div>
                    {allInputSources.filter(s => s.source === 'test').map((source) => (
                      <button
                        key={source.id}
                        onClick={() => {
                          selectSource(node.id, source.source, source.id);
                          setIsDropdownOpen(false);
                        }}
                        className="w-full px-2 py-1 text-xs text-left hover:bg-muted/50 flex items-center justify-between"
                      >
                        <span>{source.name}</span>
                        {source.source === 'test' && source.status && getStatusIcon(source.status)}
                      </button>
                    ))}
                  </>
                )}
                {executionInputSources.length > 0 && (
                  <>
                    <div className="px-2 py-0.5 text-[10px] font-semibold text-muted-foreground border-t">PREVIOUS EXECUTIONS</div>
                    {executionInputSources.map((source) => (
                      <button
                        key={source.id}
                        onClick={() => {
                          selectSource(node.id, source.source, source.id);
                          setIsDropdownOpen(false);
                        }}
                        className="w-full px-2 py-1 text-xs text-left hover:bg-muted/50 flex items-center justify-between"
                      >
                        <div className="flex items-center gap-1.5">
                          <span>{source.name}</span>
                          {source.source === 'execution' && (
                            <span className="text-[10px] text-muted-foreground">
                              ({new Date(source.timestamp).toLocaleTimeString()})
                            </span>
                          )}
                        </div>
                        {source.source === 'execution' && getStatusIcon(source.status)}
                      </button>
                    ))}
                  </>
                )}
              </div>
            )}
          </div>
        )}

        {/* Compact Action Buttons */}
        <button
          className="px-1.5 py-0.5 text-xs font-medium bg-blue-600 hover:bg-blue-700 disabled:bg-blue-600/50 text-white rounded flex items-center gap-0.5 shrink-0"
          disabled={!currentSource}
          onClick={async () => {
            if (!currentSource) return;
            const activeWorkflow = sdk.workflows.getActive();
            if (!activeWorkflow) return;
            await sdk.executions.start(activeWorkflow.id, currentSource.inputs, { startFromNodeId: node.id });
          }}
        >
          <Play className="w-2.5 h-2.5" />
          Run from here
        </button>
        <button
          className="px-1.5 py-0.5 text-xs font-medium bg-green-600 hover:bg-green-700 disabled:bg-green-600/50 text-white rounded flex items-center gap-0.5 shrink-0"
          disabled={!currentSource}
          title="Replay this node only"
          onClick={async () => {
            if (!currentSource) return;
            // TODO: Implement single-node replay
            alert('Replay node - Coming soon!\n\nThis will re-execute just this single node with the selected inputs.');
          }}
        >
          <RefreshCw className="w-2.5 h-2.5" />
          Replay
        </button>
      </div>

      {/* Node Type Badge */}
      <div className="flex items-center gap-1.5 py-1 px-1.5 bg-muted/30 rounded">
        <Terminal className="w-3 h-3 text-muted-foreground" />
        <span className="text-xs font-medium">Type:</span>
        <span className="text-xs font-mono text-muted-foreground">{node.type}</span>
      </div>

      {/* Input/Output Grid */}
      <div className="grid grid-cols-2 gap-2">
        {/* Input */}
        <div>
          <div className="flex items-center gap-1 mb-1">
            <FileInput className="w-3 h-3 text-muted-foreground" />
            <span className="text-[11px] font-semibold text-muted-foreground">Input</span>
          </div>
          <pre className="bg-muted p-1.5 rounded text-[10px] overflow-auto max-h-32 font-mono">
            {JSON.stringify(displayedInputs, null, 2)}
          </pre>
        </div>

        {/* Output */}
        <div>
          <div className="flex items-center gap-1 mb-1">
            <FileOutput className="w-3 h-3 text-muted-foreground" />
            <span className="text-[11px] font-semibold text-muted-foreground">Output</span>
          </div>
          {displayedOutputs ? (
            <pre className="bg-muted p-1.5 rounded text-[10px] overflow-auto max-h-32 font-mono">
              {JSON.stringify(displayedOutputs, null, 2)}
            </pre>
          ) : (
            <div className="bg-muted/30 border border-dashed border-muted rounded p-1.5 text-[10px] text-muted-foreground italic">
              {execution?.state === 'running' ? 'Waiting...' : 'No output'}
            </div>
          )}
        </div>
      </div>

      {/* Logs Section */}
      <div>
        <div className="flex items-center gap-1 mb-1">
          <ScrollText className="w-3 h-3 text-muted-foreground" />
          <span className="text-[11px] font-semibold text-muted-foreground">Logs</span>
        </div>
        {logs.length > 0 ? (
          <div className="space-y-0.5">
            {logs.map((log, index) => (
              <div key={index} className="text-[10px] font-mono bg-muted p-1.5 rounded">
                <span className="text-muted-foreground">
                  {new Date(log.timestamp).toLocaleTimeString()}
                </span>
                {' - '}
                <span
                  className={
                    log.level === 'error'
                      ? 'text-red-600 dark:text-red-400'
                      : log.level === 'warn'
                        ? 'text-yellow-600 dark:text-yellow-400'
                        : 'text-foreground'
                  }
                >
                  [{log.level.toUpperCase()}]
                </span>
                {' '}
                {log.message}
              </div>
            ))}
          </div>
        ) : (
          <div className="bg-muted/30 border border-dashed border-muted rounded p-3 text-center">
            <Terminal className="w-6 h-6 text-muted-foreground mx-auto mb-1 opacity-50" />
            <p className="text-[10px] text-muted-foreground">No logs emitted</p>
          </div>
        )}
      </div>
    </div>
  );
}
