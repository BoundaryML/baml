'use client';
// import * as vscode from 'vscode'

import { vscode } from '@baml/playground-common';
import { diagnosticsAtom, filesAtom, wasmAtom } from '@baml/playground-common';
import {
  flashRangesAtom,
  runtimeStateAtom,
  selectedFunctionAtom,
  selectedTestcaseAtom,
  updateCursorAtom,
} from '@baml/playground-common';
import { useRunBamlTests } from '@baml/playground-common';
import { orchIndexAtom } from '@baml/playground-common';
import { useDebounceCallback } from '@react-hook/debounce';
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { AlertTriangle, XCircle } from 'lucide-react';
import { CheckCircle } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { vscodeLocalStorageStore } from './JotaiProvider';
import { type BamlConfigAtom, bamlConfig } from './bamlConfig';
import { ErrorWarningDialog } from '../components/ErrorWarningDialog';
import { useState } from 'react';

export const hasClosedEnvVarsDialogAtom = atomWithStorage<boolean>(
  'has-closed-env-vars-dialog',
  false,
  vscodeLocalStorageStore,
);
export const bamlCliVersionAtom = atom<string | null>(null);

export const showIntroToChecksDialogAtom = atom(false);
export const hasClosedIntroToChecksDialogAtom = atomWithStorage<boolean>(
  'has-closed-intro-to-checks-dialog',
  false,
  vscodeLocalStorageStore,
);

export const versionAtom = atom((get) => {
  const wasm = get(wasmAtom);

  if (wasm === undefined) {
    return 'Loading...';
  }

  return wasm.version();
});

export const numErrorsAtom = atom((get) => {
  const errors = get(diagnosticsAtom);

  const warningCount = errors.filter((e: any) => e.type === 'warning').length;

  return { errors: errors.length - warningCount, warnings: warningCount };
});

const ErrorCount: React.FC<{ onClick?: () => void }> = ({ onClick }) => {
  const { errors, warnings } = useAtomValue(numErrorsAtom);
  if (errors === 0 && warnings === 0) {
    return (
      <div className="flex flex-row gap-1 items-center text-green-600">
        <CheckCircle size={12} />
      </div>
    );
  }
  if (errors === 0) {
    return (
      <button
        type="button"
        onClick={onClick}
        className="flex flex-row gap-1 items-center text-yellow-600 hover:underline focus:outline-none"
        title="Show warnings"
      >
        {warnings} <AlertTriangle size={12} />
      </button>
    );
  }
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex flex-row gap-1 items-center text-red-600 hover:underline focus:outline-none"
      title="Show errors and warnings"
    >
      {errors} <XCircle size={12} /> {warnings} <AlertTriangle size={12} />
    </button>
  );
};

export const isConnectedAtom = atom(true);

// const ConnectionStatus: React.FC = () => {
//   const isConnected = useAtomValue(isConnectedAtom)

//   if (isConnected || vscode.isVscode()) return null

//   return (
//     <div className='fixed top-0 left-0 right-0 bg-red-600 text-white p-2 flex items-center justify-between z-50'>
//       <div className='flex items-center gap-2'>
//         <XCircle size={16} />
//         <span>Disconnected from LSP server</span>
//       </div>
//       <button
//         onClick={() => window.location.reload()}
//         type='button'
//         className='px-3 py-1 bg-white text-red-600 rounded hover:bg-red-50 transition-colors'
//       >
//         Reconnect
//       </button>
//     </div>
//   )
// }

// Deep equality check for objects
const deepEqual = (obj1: any, obj2: any): boolean => {
  if (obj1 === obj2) return true;
  if (!obj1 || !obj2) return false;
  if (typeof obj1 !== 'object' || typeof obj2 !== 'object') return false;
  
  const keys1 = Object.keys(obj1);
  const keys2 = Object.keys(obj2);
  
  if (keys1.length !== keys2.length) return false;
  
  for (const key of keys1) {
    if (!keys2.includes(key)) return false;
    if (!deepEqual(obj1[key], obj2[key])) return false;
  }
  
  return true;
};

// We don't use ASTContext.provider because we should the default value of the context
export const EventListener: React.FC = () => {
  const updateCursor = useSetAtom(updateCursorAtom)
  const setFiles = useSetAtom(filesAtom)
  const currentFilesRef = useRef<Record<string, string>>({})
  
  // Wrap setFiles to only update if files actually changed
  const setFilesIfChanged = useDebounceCallback((newFiles: Record<string, string>) => {
    if (!deepEqual(currentFilesRef.current, newFiles)) {
      currentFilesRef.current = newFiles;
      setFiles(newFiles);
    }
  }, 20, true)
  
  const setFlashRanges = useSetAtom(flashRangesAtom)
  const setIsConnected = useSetAtom(isConnectedAtom)
  const isVSCodeWebview = vscode.isVscode()

  const [selectedFunc, setSelectedFunction] = useAtom(selectedFunctionAtom)
  const [selectedTestcase, setSelectedTestcase] = useAtom(selectedTestcaseAtom)
  const setBamlConfig = useSetAtom(bamlConfig)
  const [bamlCliVersion, setBamlCliVersion] = useAtom(bamlCliVersionAtom)
  const runBamlTests = useRunBamlTests()
  const wasm = useAtomValue(wasmAtom)
  const runtimeState = useAtomValue(runtimeStateAtom)


  const setOrchestratorIndex = useSetAtom(orchIndexAtom)

  useEffect(() => {
    if (selectedFunc) {
      // todo: maybe we use a derived atom to reset it. But for now this useeffect works.
      setOrchestratorIndex(0)
    }
  }, [selectedFunc])
  // console.log('selectedFunc', selectedFunc)

  useEffect(() => {
    // Only open websocket if not in VSCode webview
    if (isVSCodeWebview) {
      setIsConnected(true)
      return
    }

    const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
    const ws = new WebSocket(`${scheme}://${window.location.host}/ws`)

    ws.onopen = () => {
      console.debug('WebSocket Opened')
      setIsConnected(true)
    }
    ws.onmessage = (e) => {
      console.debug('Websocket recieved message!')
      try {
        const payload = JSON.parse(e.data)
        window.postMessage(payload, '*')
      } catch (err) {
        console.error('invalid WS payload', err)
      }
    }
    ws.onclose = () => {
      console.debug('WebSocket Closed')
      setIsConnected(false)
    }
    ws.onerror = () => {
      console.error('WebSocket error')
      setIsConnected(false)
    }

    return () => ws.close()
  }, [setIsConnected, isVSCodeWebview])


  useEffect(() => {
    // console.debug('adding event listener');
    console.debug('selectedFunc', selectedFunc, 'selectedTestcase', selectedTestcase);
    const fn = (
      event: MessageEvent<
        | {
            command: 'add_project';
            content: {
              root_path: string;
              files: Record<string, string>;
            };
          }
        | {
            command: 'remove_project';
            content: {
              root_path: string;
            };
          }
        | {
            command: 'set_flashing_regions';
            content: {
              spans: {
                file_path: string;
                start_line: number;
                start: number;
                end_line: number;
                end: number;
              }[];
            };
          }
        | {
            command: 'select_function';
            content: {
              root_path: string;
              function_name: string;
            };
          }
        | {
            command: 'update_cursor';
            content: {
              cursor: {
                fileName: string;
                fileText: string;
                line: number;
                column: number;
              };
            };
          }
        | {
            command: 'port_number';
            content: {
              port: number;
            };
          }
        | {
            command: 'baml_cli_version';
            content: string;
          }
        | {
            command: 'baml_settings_updated';
            content: BamlConfigAtom;
          }
        | {
            command: 'run_test';
            content: { test_name: string };
          }
      >,
    ) => {
      const { command, content } = event.data;
      console.debug('command', command);

      switch (command) {
        case 'add_project':
          if (content?.root_path) {
            // console.debug('add_project', content.root_path);
            setFilesIfChanged(
              Object.fromEntries(
                Object.entries(content.files).map(([name, content]) => [
                  name,
                  content,
                ]),
              ),
            );
          } else {
            console.error('add_project: no root_path');
          }
          break;

        case 'set_flashing_regions':
          console.debug('DEBUG set_flashing_regions', content);
          setFlashRanges(
            content.spans.map((span) => ({
              filePath: span.file_path,
              startLine: span.start_line,
              startCol: span.start,
              endLine: span.end_line,
              endCol: span.end,
            })),
          );
          break;

        case 'select_function':
          console.debug('select_function', content);
          
          // Handle VSCode sending 'default' or non-existent function names
          let functionToSelect = content.function_name;
          const func = runtimeState.functions.find(f => f.name === content.function_name);
          
          if (!func) {
            // Function doesn't exist, select the first available function
            const firstFunc = runtimeState.functions[0];
            if (firstFunc) {
              functionToSelect = firstFunc.name;
              console.debug('Function not found, selecting first function:', functionToSelect);
            }
          }
          
          setSelectedFunction(functionToSelect);
          
          // Reset test case to first for the selected function, or undefined if no test cases
          const selectedFunction = runtimeState.functions.find(f => f.name === functionToSelect);
          if (selectedFunction && selectedFunction.test_cases.length > 0) {
            setSelectedTestcase(selectedFunction.test_cases[0]?.name);
          } else {
            setSelectedTestcase(undefined);
          }
          break;
        case 'update_cursor':
          if ('cursor' in content) {
            console.debug('update_cursor', content.cursor.fileName.split('/').pop());
            updateCursor(content.cursor);
          }
          break;
        case 'baml_settings_updated':
          console.debug('baml_settings_updated', content);
          setBamlConfig(content);
          break;
        case 'baml_cli_version':
          console.debug('baml_cli_version', content);
          setBamlCliVersion(content);
          break;

        case 'remove_project':
          setFiles({});
          break;

        case 'run_test':
          if (selectedFunc) {
            setSelectedTestcase(content.test_name);
            runBamlTests([
              { functionName: selectedFunc, testName: content.test_name },
            ]);
          } else {
            console.error('No function selected');
          }
          // run([content.test_name])
          // setShowTests(true)
          // setClientGraph(false)
          break;
      }
    };

    window.addEventListener('message', fn);

    return () => window.removeEventListener('message', fn);
    // If we dont add the jotai atom callbacks here like setRunningTests, this will call an old version of the atom (e.g. runTests which may have undefined dependencies).
  }, [selectedFunc, setSelectedTestcase, setSelectedFunction]);

  useEffect(() => {
    if (wasm) {
      console.debug('wasm ready!')
      try {
        vscode.markInitialized()
      } catch (e) {
        console.error('Error marking initialized', e)
      }
    }
  }, [wasm])

  const version = useAtomValue(versionAtom);
  const [showDialog, setShowDialog] = useState(false);

  return (
    <>
      {/* <ConnectionStatus /> */}
      <div className="flex flex-row gap-2 text-xs bg-transparent items-center">
        <div className="pr-4 whitespace-nowrap">
          {bamlCliVersion && `baml-cli ${bamlCliVersion}`}
        </div>
        <ErrorCount onClick={() => setShowDialog(true)} />
        <ErrorWarningDialog open={showDialog} onOpenChange={setShowDialog} />
        <span className="text-muted-foreground text-[10px]">
          VSCode Runtime Version: {version}
        </span>
      </div>
    </>
  );
};
