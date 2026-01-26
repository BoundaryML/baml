"use client";

import { vscode } from "../shared/baml-project-panel/vscode";
import { useDebounceCallback } from "@react-hook/debounce";
import { atom, useSetAtom } from "jotai";
import { atomWithStorage } from "jotai/utils";
import { useEffect } from "react";
import { vscodeLocalStorageStore } from "./JotaiProvider";
import { bamlConfig } from "./bamlConfig";
import { VscodeToWebviewCommand } from "./vscode-to-webview-rpc";
import { handleIDEMessage, handleLSPMessage } from "./message-handlers";
import { useBAMLSDK } from "../sdk";

// ============================================================================
// EventListener-specific atoms (not managed by SDK)
// ============================================================================

export const bamlCliVersionAtom = atom<string | null>(null);

export const showIntroToChecksDialogAtom = atom(false);

export const hasClosedIntroToChecksDialogAtom = atomWithStorage<boolean>(
  "has-closed-intro-to-checks-dialog",
  false,
  vscodeLocalStorageStore,
);

export const isConnectedAtom = atom(true);

// ============================================================================
// EventListener Component
// ============================================================================

export const EventListener: React.FC = () => {
  // Get SDK instance (guaranteed to be initialized by provider)
  const sdk = useBAMLSDK();
  const isVSCodeWebview = vscode.isVscode();

  // Non-core state atoms (not managed by SDK)
  const setBamlCliVersion = useSetAtom(bamlCliVersionAtom);
  const setBamlConfig = useSetAtom(bamlConfig);

  // Mark as initialized when component mounts (SDK is ready)
  useEffect(() => {
    console.debug("SDK ready, marking initialized");
    try {
      vscode.markInitialized();
    } catch (e) {
      console.error("Error marking initialized", e);
    }
  }, []);

  // Platform quirk: Debounce file updates to prevent excessive WASM recompilation
  const debouncedUpdateFiles = useDebounceCallback(
    (files: Record<string, string>) => {
      console.debug('[EventListener] Debounced file update');
      const simpleBamlEntry = Object.entries(files).find(([name]) => name.endsWith('simple.baml'));
      if (simpleBamlEntry) {
        const [name, content] = simpleBamlEntry;
      }
      sdk.files.update(files);
    },
    50,
    true // Leading edge
  );

  const debouncedUpdateCursor = useDebounceCallback(
    (cursor: { fileName: string; line: number; column: number }) => {
      console.debug('[EventListener] Debounced cursor update');
      sdk.navigation.updateCursor(cursor);
    },
    50,
    true // Leading edge
  );

  useEffect(() => {
    // Only open websocket if not in VSCode webview
    if (isVSCodeWebview) {
      return;
    }

    const scheme = window.location.protocol === "https:" ? "wss" : "ws";
    const ws = new WebSocket(`${scheme}://${window.location.host}/ws`);

    ws.onmessage = (e) => {
      try {
        const payload = JSON.parse(e.data);
        window.postMessage(payload, "*");
      } catch (err) {
        console.error("invalid WS payload", err);
      }
    };

    return () => ws.close();
  }, [isVSCodeWebview]);

  // Main message handler - routes IDE/LSP messages to SDK methods
  useEffect(() => {
    const handler = async (event: MessageEvent<VscodeToWebviewCommand>) => {
      const { source, payload } = event.data || {};

      // Ignore messages that don't have the expected structure
      if (!source) {
        return;
      }
      if (typeof source === 'string' && source.startsWith('react-devtools-')) {
        // Ignore noisy React DevTools bridge chatter
        return;
      }

      console.debug('[EventListener] Handling command:', { source, payload });

      try {
        switch (source) {
          case 'ide_message':
            // Handle IDE messages via SDK
            await handleIDEMessage(
              sdk,
              payload,
              debouncedUpdateCursor,
              setBamlCliVersion,
              setBamlConfig
            );
            break;

          case 'lsp_message':
            // Handle LSP messages via SDK
            await handleLSPMessage(sdk, payload, debouncedUpdateFiles, setBamlConfig);
            break;

          default:
            console.warn('[EventListener] Unknown message source:', source);
        }
      } catch (error) {
        // Log error but don't crash EventListener - continue processing other messages
        console.error('[EventListener] Error handling message:', {
          source,
          payload,
          error,
        });
      }
    };

    window.addEventListener('message', handler);

    return () => window.removeEventListener('message', handler);
  }, [sdk, debouncedUpdateFiles, debouncedUpdateCursor, setBamlCliVersion, setBamlConfig]);

  return (
    <>
      {/* EventListener handles background events - no UI needed since StatusBar handles display */}
    </>
  );
};
