'use client';
// import * as vscode from 'vscode'

import { vscode } from '@baml/playground-common';
import {
  diagnosticsAtom, filesAtom, wasmAtom,
  selectedFunctionAtom,
  selectedTestcaseAtom,
  updateCursorAtom,
} from '@baml/playground-common';
import { useRunBamlTests } from '@baml/playground-common';
import { orchIndexAtom } from '@baml/playground-common';
import { useDebounceCallback } from '@react-hook/debounce';
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai';
import { atomWithStorage } from 'jotai/utils';
import { useEffect } from 'react';
import { vscodeLocalStorageStore } from './JotaiProvider';
import { type BamlConfigAtom, bamlConfig } from './bamlConfig';
import { VscodeToWebviewCommand } from './vscode-to-webview-rpc';

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

export const isConnectedAtom = atom(true);

export const EventListener: React.FC = () => {
  const updateCursor = useSetAtom(updateCursorAtom)
  const [bamlFileMap, setBamlFileMap] = useAtom(filesAtom)
  const debouncedSetBamlFileMap = useDebounceCallback(setBamlFileMap, 50, true)
  const isVSCodeWebview = vscode.isVscode()

  const [selectedFunc, setSelectedFunction] = useAtom(selectedFunctionAtom)
  const [selectedTestcase, setSelectedTestcase] = useAtom(selectedTestcaseAtom)
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

  useEffect(() => {
    // Only open websocket if not in VSCode webview
    if (isVSCodeWebview) {
      return
    }

    const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws'
    const ws = new WebSocket(`${scheme}://${window.location.host}/ws`)

    ws.onmessage = (e) => {
      try {
        const payload = JSON.parse(e.data)
        window.postMessage(payload, '*')
      } catch (err) {
        console.error('invalid WS payload', err)
      }
    }

    return () => ws.close()
  }, [isVSCodeWebview])

  useEffect(() => {
    const fn = (event: MessageEvent<VscodeToWebviewCommand>) => {
      const { source, payload } = event.data;
      console.debug('EventListener handling command', { source, payload });

      switch (source) {
        case 'ide_message':
          const { command, content } = payload;
          switch (command) {
            case 'update_cursor':
              updateCursor(content);
              break;
            case 'baml_settings_updated':
              setBamlConfig(content);
              break;
            case 'baml_cli_version':
              setBamlCliVersion(content);
              break;
          }
          break;
        case 'lsp_message':
          const { method, params } = payload;
          switch (method) {
            case 'baml_settings_updated':
              setBamlConfig(({ config: prevConfig, ...prevRest }) => {
                const newConfig = { ...prevRest, config: { ...prevConfig, ...params } };
                console.debug('baml_settings_updated', { prevConfig, params, newConfig });
                return newConfig
              });
              break;
            case 'runtime_updated':
              debouncedSetBamlFileMap(
                Object.fromEntries(
                  Object.entries(params.files).map(([name, content]) => [
                    name,
                    content,
                  ]),
                ),
              );
              break;
            case 'workspace/executeCommand':
              const { command } = params;
              switch (command) {
                case 'baml.openBamlPanel': {
                  const [args] = params.arguments;
                  setSelectedFunction(args.functionName);
                  break;
                }
                case 'baml.runBamlTest': {
                  const [args] = params.arguments;
                  setSelectedFunction(args.functionName);
                  setSelectedTestcase(args.testCaseName);

                  // NB(sam): without this timeout, jetbrains hits "recursive use of an object"
                  setTimeout(() => {
                    runBamlTests([
                      { functionName: args.functionName, testName: args.testCaseName },
                    ]);
                  }, 1000);
                  break;
                }
              }
              break;
            case 'textDocument/codeAction': {
              const { textDocument, range } = params;
              // TODO: this needs testing for escaped file paths!
              const fileName = textDocument.uri.replace('file://', '');
              updateCursor({
                fileName,
                line: range.start.line,
                column: range.start.character,
              });
              break;
            }
          }
          break;
      }
    }

    window.addEventListener('message', fn);

    return () => window.removeEventListener('message', fn);
    // If we dont add the jotai atom callbacks here like setRunningTests, this will call an old version of the atom (e.g. runTests which may have undefined dependencies).
  }, [selectedFunc, runBamlTests, updateCursor, setSelectedFunction, setSelectedTestcase, bamlFileMap]);

  return (
    <>
      {/* EventListener handles background events - no UI needed since StatusBar handles display */}
    </>
  );
};
