/**
 * Debug Panel Component
 *
 * Simulates clicking on functions and tests in BAML files
 * to test how the app reacts to code navigation events
 */

import { useAtom, useAtomValue } from 'jotai';
import { Play, FileCode, Folder, FolderOpen, ChevronRight, ChevronDown, Plus, Edit, GitBranch, RefreshCw, Layers, CornerDownLeft, Square } from 'lucide-react';
import { useState, useEffect, useMemo } from 'react';
import { bamlFilesAtom, SelectionQuery } from '../../../sdk/atoms/core.atoms';
import type { BAMLTest, CodeClickEvent, BAMLFile } from '../../../sdk/types';
import { useBAMLSDK, useNavigation } from '../../../sdk/hooks';
import type { VscodeToWebviewCommand } from '../../../baml_wasm_web/vscode-to-webview-rpc';
import { useRunBamlTests } from '../../../shared/baml-project-panel/playground-panel/prompt-preview/test-panel/test-runner';
import { unifiedSelectionAtom } from '../../../shared/baml-project-panel/playground-panel/unified-atoms';
import type { FunctionWithCallGraph, NodeType } from '../../../sdk/interface';
import type { NavigationInput } from '../../../sdk/navigation';

type DebugNodeType = NodeType | 'workflow' | 'block';
type FunctionEventType = Extract<CodeClickEvent, { type: 'function' }>['functionType'];

interface DebugNode {
  id: string;
  label: string;
  nodeType: DebugNodeType;
  filePath: string;
  origin: 'function' | 'workflow-node';
  workflowId?: string;
}

function mapNodeTypeToEventType(nodeType: DebugNodeType): FunctionEventType {
  switch (nodeType) {
    case 'workflow':
      return 'workflow';
    case 'llm_function':
      return 'llm_function';
    case 'conditional':
      return 'conditional';
    case 'loop':
      return 'loop';
    case 'group':
      return 'group';
    case 'return':
      return 'return';
    case 'block':
      return 'block';
    default:
      return 'function';
  }
}

function buildNodesByFile(
  files: BAMLFile[],
  workflows: FunctionWithCallGraph[],
): Record<string, DebugNode[]> {
  const workflowsByFile = workflows.reduce<Map<string, FunctionWithCallGraph[]>>((acc, workflow) => {
    const filePath = workflow.filePath || workflow.span?.filePath;
    if (!filePath) {
      return acc;
    }
    if (!acc.has(filePath)) {
      acc.set(filePath, []);
    }
    acc.get(filePath)!.push(workflow);
    return acc;
  }, new Map());

  const nodesByFile: Record<string, DebugNode[]> = {};

  const addNode = (filePath: string, node: DebugNode) => {
    if (!nodesByFile[filePath]) {
      nodesByFile[filePath] = [];
    }
    const existingIndex = nodesByFile[filePath].findIndex((n) => n.id === node.id);
    if (existingIndex === -1) {
      nodesByFile[filePath].push(node);
    } else {
      // Prefer workflow metadata (gives us nodeType + workflow id)
      const existing = nodesByFile[filePath][existingIndex];
      if (!existing) {
        nodesByFile[filePath][existingIndex] = node;
        return;
      }
      nodesByFile[filePath][existingIndex] = {
        ...existing,
        ...node,
        label: node.label || existing.label,
        origin: existing.origin === 'workflow-node' ? existing.origin : node.origin,
      };
    }
  };

  for (const file of files) {
    for (const func of file.functions) {
      addNode(file.path, {
        id: func.name,
        label: func.name,
        nodeType: func.type === 'workflow' ? 'workflow' : func.type,
        filePath: file.path,
        origin: 'function',
      });
    }

    const relatedWorkflows = workflowsByFile.get(file.path) ?? [];
    for (const workflow of relatedWorkflows) {
      for (const node of workflow.nodes ?? []) {
        if (!node?.id) continue;
        addNode(file.path, {
          id: node.id,
          label: node.label || node.id,
          nodeType: node.type ?? 'function',
          filePath: file.path,
          origin: 'workflow-node',
          workflowId: workflow.id,
        });
      }
    }
  }

  // Ensure workflows without explicit file entries still surface their nodes
  for (const workflow of workflows) {
    const filePath = workflow.filePath || workflow.span?.filePath;
    if (!filePath) continue;
    if (!nodesByFile[filePath]) {
      nodesByFile[filePath] = [];
    }
    for (const node of workflow.nodes ?? []) {
      if (!node?.id) continue;
      const exists = nodesByFile[filePath].some((n) => n.id === node.id);
      if (!exists) {
        nodesByFile[filePath].push({
          id: node.id,
          label: node.label || node.id,
          nodeType: node.type ?? 'function',
          filePath,
          origin: 'workflow-node',
          workflowId: workflow.id,
        });
      }
    }
  }

  Object.values(nodesByFile).forEach((nodes) => {
    nodes.sort((a, b) => a.label.localeCompare(b.label));
  });

  return nodesByFile;
}

function NodeIcon({ nodeType }: { nodeType: DebugNodeType }) {
  const className = 'w-3 h-3 text-muted-foreground';
  switch (nodeType) {
    case 'workflow':
    case 'conditional':
      return <GitBranch className={className} />;
    case 'loop':
      return <RefreshCw className={className} />;
    case 'group':
      return <Layers className={className} />;
    case 'return':
      return <CornerDownLeft className={className} />;
    case 'block':
      return <Square className={className} />;
    default:
      return <FileCode className={className} />;
  }
}

function NodeTag({ nodeType }: { nodeType: DebugNodeType }) {
  if (nodeType === 'llm_function') {
    return (
      <span className="ml-auto text-[8px] px-1 py-0.5 bg-purple-100 dark:bg-purple-900 text-purple-700 dark:text-purple-300 rounded">
        LLM
      </span>
    );
  }
  if (nodeType === 'conditional') {
    return (
      <span className="ml-auto text-[8px] px-1 py-0.5 bg-amber-100 dark:bg-amber-900 text-amber-700 dark:text-amber-200 rounded">
        IF
      </span>
    );
  }
  if (nodeType === 'loop') {
    return (
      <span className="ml-auto text-[8px] px-1 py-0.5 bg-green-100 dark:bg-green-900 text-green-700 dark:text-green-300 rounded">
        LOOP
      </span>
    );
  }
  if (nodeType === 'group') {
    return (
      <span className="ml-auto text-[8px] px-1 py-0.5 bg-slate-100 dark:bg-slate-900 text-slate-700 dark:text-slate-200 rounded">
        GROUP
      </span>
    );
  }
  return null;
}

export function DebugPanel() {
  const sdk = useBAMLSDK();
  const { runTests: runBamlTests } = useRunBamlTests();
  const [bamlFiles, setBAMLFiles] = useAtom(bamlFilesAtom);
  const navigate = useNavigation();
  const selection = useAtomValue(unifiedSelectionAtom);
  const [expandedFiles, setExpandedFiles] = useState<Set<string>>(new Set());
  const [workflows, setWorkflows] = useState<FunctionWithCallGraph[]>([]);

  // Load BAML files on mount
  useEffect(() => {
    console.log('[DebugPanel] Mounted, loading BAML files...');
    const files = sdk.diagnostics.getBAMLFiles();
    // console.log('[DebugPanel] Loaded files:', files);
    setBAMLFiles(files);
    // Expand all files by default
    setExpandedFiles(new Set(files.map((f: any) => f.path)));
  }, [sdk, setBAMLFiles]);

  useEffect(() => {
    const allWorkflows = sdk.workflows.getAll();
    setWorkflows(allWorkflows);
  }, [sdk, bamlFiles]);

  const nodesByFile = useMemo(
    () => buildNodesByFile(bamlFiles ?? [], workflows),
    [bamlFiles, workflows]
  );

  console.log('[DebugPanel] Rendering with', bamlFiles.length, 'files');

  const toggleFile = (path: string) => {
    setExpandedFiles(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const handleNodeClick = (node: DebugNode) => {
    const input: NavigationInput = {
      kind: node.workflowId ? 'node' : 'function',
      source: 'debug-panel',
      timestamp: Date.now(),
      functionName: node.id,
      workflowId: node.workflowId,
      nodeId: node.workflowId ? node.id : undefined,
      functionType: mapNodeTypeToEventType(node.nodeType),
    };
    console.log('🔍 Debug panel node click:', { ...input, origin: node.origin });
    navigate(input);
  };

  const handleTestClick = (test: BAMLTest) => {
    const input: NavigationInput = {
      kind: 'test',
      source: 'debug-panel',
      timestamp: Date.now(),
      testName: test.name,
      functionName: test.functionName,
    };
    console.log('🔍 Simulated test click:', input);
    navigate(input);
  };

  const handleTestRun = async (test: BAMLTest, e: React.MouseEvent) => {
    e.stopPropagation(); // Prevent triggering the test click
    console.log('▶️ Running test:', test.name, '→', test.functionName);

    // Run the test - SDK will automatically set selection and trigger scroll
    // Don't call handleTestClick first, as that would set selection before test history exists,
    // preventing the scroll from triggering when selection changes
    await runBamlTests([{ functionName: test.functionName, testName: test.name }]);
  };

  const isNodeActive = (node: DebugNode) => {
    return SelectionQuery.isNodeActive(selection, node.id);
  };

  const isTestActive = (test: BAMLTest) => {
    return SelectionQuery.isTestActive(selection, test);
  };

  // Simulate adding a new file with CheckAvailability function
  const handleAddNewFile = () => {
    console.log('🆕 Simulating adding new file with CheckAvailability function');

    const newFileContent = `// New functions for testing
function CheckAvailability2(day: string) -> bool {
  client GPT4o

  prompt #"
    Is the office open on {{ day }}?
    The office is open Monday through Friday.

    Return true if open, false if closed.

    {{ ctx.output_format }}
  "#
}

function CountItems2(text: string) -> int {
  client GPT4o

  prompt #"
    Count how many items are mentioned in the following text:

    {{ text }}

    {{ ctx.output_format }}
  "#
}

test CheckAvailabilityTest2 {
  functions [CheckAvailability2]
  args {
    day "Monday"
  }
}

test CountItemsTest2 {
  functions [CountItems2]
  args {
    text "apples, oranges, bananas"
  }
}`;

    // Get current files from SDK
    const currentFiles = sdk.files.getCurrent();

    // Add the new file
    const updatedFiles = {
      ...currentFiles,
      'baml_src/additional_functions.baml': newFileContent,
    };

    // Post message to EventListener (simulates IDE file change)
    const message: VscodeToWebviewCommand = {
      source: 'lsp_message',
      payload: {
        method: 'runtime_updated',
        params: {
          root_path: '/fake/root',
          files: updatedFiles,
        },
      },
    };

    console.log('📨 Posting runtime_updated message with new file');
    window.postMessage(message, '*');
  };

  // Simulate modifying an existing function to add a sentence
  const handleModifyFunction = () => {
    console.log('✏️ Simulating modifying ExtractResume function');

    const currentFiles = sdk.files.getCurrent();

    // Find and modify the main.baml file
    const mainFile = currentFiles['baml_src/main.baml'];
    if (!mainFile) {
      console.warn('⚠️ main.baml not found, cannot modify');
      return;
    }

    // Add a sentence to the ExtractResume prompt
    // Edit the ExtractResume prompt and add the date at the end
    const now = new Date();
    const dateTime = now.toISOString().replace('T', ' ').slice(0, 19); // "YYYY-MM-DD HH:MM:SS"
    const modifiedFile = mainFile.replace(
      'Parse the following resume and return a structured representation of the data in the schema below.',
      `Parse the following resume and return a structured representation of the data in the schema below.

Please be thorough and accurate in your extraction.

Date: ${dateTime}`
    );

    const updatedFiles = {
      ...currentFiles,
      'baml_src/main.baml': modifiedFile,
    };

    // Post message to EventListener (simulates IDE file edit)
    const message: VscodeToWebviewCommand = {
      source: 'lsp_message',
      payload: {
        method: 'runtime_updated',
        params: {
          root_path: '/fake/root',
          files: updatedFiles,
        },
      },
    };

    console.log('📨 Posting runtime_updated message with modified file');
    window.postMessage(message, '*');
  };

  return (
    <div className="absolute bottom-2 right-0 z-[1000] w-[200px] bg-card border border-border rounded-md shadow-lg max-h-[400px] overflow-y-auto">
      {/* Header */}
      <div className="sticky top-0 bg-card border-b border-border px-2 py-1">
        <h3 className="text-[10px] font-semibold text-foreground uppercase tracking-wide mb-1">Debug</h3>

        {/* Test Buttons */}
        <div className="flex gap-1 mb-1">
          <button
            onClick={handleAddNewFile}
            className="flex items-center gap-1 px-1.5 py-0.5 text-[9px] bg-green-600 hover:bg-green-700 text-white rounded transition-colors"
            title="Add new file with CheckAvailability & CountItems functions"
          >
            <Plus className="w-2.5 h-2.5" />
            <span>Add File</span>
          </button>
          <button
            onClick={handleModifyFunction}
            className="flex items-center gap-1 px-1.5 py-0.5 text-[9px] bg-blue-600 hover:bg-blue-700 text-white rounded transition-colors"
            title="Modify ExtractResume function prompt"
          >
            <Edit className="w-2.5 h-2.5" />
            <span>Edit Func</span>
          </button>
        </div>
      </div>

      {/* File List */}
      <div className="p-1">
        {bamlFiles.length === 0 ? (
          <div className="text-[10px] text-muted-foreground p-2 text-center">
            No BAML files loaded yet
          </div>
        ) : (
          bamlFiles.map((file) => {
            const isExpanded = expandedFiles.has(file.path);
            const nodes = nodesByFile[file.path] ?? [];
            return (
              <div key={file.path} className="mb-1">
                {/* File Header */}
                <button
                  onClick={() => toggleFile(file.path)}
                  className="w-full flex items-center gap-1 px-1 py-0.5 text-[10px] hover:bg-muted/50 rounded transition-colors"
                >
                  {isExpanded ? (
                    <ChevronDown className="w-3 h-3 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="w-3 h-3 text-muted-foreground" />
                  )}
                  {isExpanded ? (
                    <FolderOpen className="w-3 h-3 text-blue-500" />
                  ) : (
                    <Folder className="w-3 h-3 text-blue-500" />
                  )}
                  <span className="font-medium text-foreground truncate">{file.path}</span>
                </button>

                {/* File Contents */}
                {isExpanded && (
                  <div className="ml-4 mt-0.5 space-y-1">
                    {nodes.length > 0 && (
                      <div>
                        <div className="text-[9px] font-semibold text-muted-foreground uppercase tracking-wide px-1 py-0.5">
                          Nodes
                        </div>
                        {nodes.map((node) => (
                          <button
                            key={`${file.path}-${node.id}`}
                            onClick={() => handleNodeClick(node)}
                            className={`w-full flex items-center gap-1 px-1 py-0.5 text-[10px] hover:bg-muted/50 rounded transition-colors ${isNodeActive(node) ? 'bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300' : ''}`}
                          >
                            <NodeIcon nodeType={node.nodeType} />
                            <span className="truncate">{node.label}</span>
                            <NodeTag nodeType={node.nodeType} />
                          </button>
                        ))}
                      </div>
                    )}

                    {/* Tests Section */}
                    {file.tests.length > 0 && (
                      <div>
                        <div className="text-[9px] font-semibold text-muted-foreground uppercase tracking-wide px-1 py-0.5">
                          Tests
                        </div>
                        {file.tests.map((test: BAMLTest) => (
                          <div
                            key={test.name}
                            onClick={() => handleTestClick(test)}
                            className={`w-full flex items-center gap-1 px-1 py-0.5 text-[10px] hover:bg-muted/50 rounded transition-colors group cursor-pointer ${isTestActive(test) ? 'bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300' : ''
                              }`}
                          >
                            <Play className="w-3 h-3 text-muted-foreground" />
                            <span className="truncate flex-1 text-left">{test.name}</span>
                            <button
                              onClick={(e) => handleTestRun(test, e)}
                              className="opacity-0 group-hover:opacity-100 p-0.5 bg-green-600 hover:bg-green-700 text-white rounded transition-opacity"
                              title={`Run test for ${test.functionName}`}
                            >
                              <Play className="w-2.5 h-2.5" />
                            </button>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

    </div>
  );
}
