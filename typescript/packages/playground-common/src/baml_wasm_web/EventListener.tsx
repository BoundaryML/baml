'use client';
// import * as vscode from 'vscode'

import { vscode } from '@baml/playground-common';
import { diagnosticsAtom, filesAtom, wasmAtom } from '@baml/playground-common';
import {
  flashRangesAtom,
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
import { useEffect } from 'react';
import { vscodeLocalStorageStore } from './JotaiProvider';
import { type BamlConfigAtom, bamlConfig } from './bamlConfig';

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

// We don't use ASTContext.provider because we should the default value of the context
export const EventListener: React.FC = () => {
  const updateCursor = useSetAtom(updateCursorAtom)
  const setFiles = useSetAtom(filesAtom)
  const debouncedSetFiles = useDebounceCallback(setFiles, 50, true)
  const setFlashRanges = useSetAtom(flashRangesAtom)
  const setIsConnected = useSetAtom(isConnectedAtom)
  const isVSCodeWebview = vscode.isVscode()

  const [selectedFunc, setSelectedFunction] = useAtom(selectedFunctionAtom)
  const setSelectedTestcase = useSetAtom(selectedTestcaseAtom)
  const setBamlConfig = useSetAtom(bamlConfig)
  const [bamlCliVersion, setBamlCliVersion] = useAtom(bamlCliVersionAtom)
  const { runTests: runBamlTests } = useRunBamlTests()
  const wasm = useAtomValue(wasmAtom)
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

  console.debug('Websocket execution finished');

  useEffect(() => {
    console.debug('adding event listener');
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
            content: {
              function_name: string;
              test_name: string;
            };
          }
      >,
    ) => {
      const { command, content } = event.data;
      console.debug('command', command);

      switch (command) {
        case 'add_project':
          if (content?.root_path) {
            console.debug('add_project', content.root_path);
            debouncedSetFiles(
              Object.fromEntries(
                Object.entries(content.files).map(([name, content]) => [
                  name,
                  content,
                ]),
              ),
            );
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
          setSelectedFunction(content.function_name);
          break;
        case 'update_cursor':
          if ('cursor' in content) {
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
          setSelectedFunction(content.function_name);
          setSelectedTestcase(content.test_name);
          runBamlTests([
            { functionName: content.function_name, testName: content.test_name },
          ]);
          // run([content.test_name])
          // setShowTests(true)
          // setClientGraph(false)
          break;
      }
    };

    window.addEventListener('message', fn);

    return () => window.removeEventListener('message', fn);
    // If we dont add the jotai atom callbacks here like setRunningTests, this will call an old version of the atom (e.g. runTests which may have undefined dependencies).
  }, [selectedFunc, runBamlTests, updateCursor, setSelectedTestcase]);

  return (
    <>
      {/* EventListener handles background events - no UI needed since StatusBar handles display */}
    </>
  );
};
