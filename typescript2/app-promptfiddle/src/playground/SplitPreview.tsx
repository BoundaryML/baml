/**
 * SplitPreview — composition shell for the promptfiddle editor + execution panel.
 *
 * The MonacoEditor takes full screen and hosts the VS Code workbench.
 * The ExecutionPanel is rendered as a custom EditorPane tab inside that
 * workbench (opened automatically when the WASM worker is ready).
 *
 * One worker, one WASM runtime. MonacoEditor owns the worker lifecycle
 * and opens the execution panel pane via ExecutionPanelPane.ts.
 */

import type { FC } from 'react';
import { useState, useRef, useEffect } from 'react';
import { usePlayground } from './PlaygroundProvider';
import { MonacoEditor } from './MonacoEditor';

const ResetIcon: FC<{ size?: number }> = ({ size = 14 }) => (
  <svg
    aria-hidden="true"
    fill="none"
    height={size}
    stroke="currentColor"
    strokeLinecap="round"
    strokeLinejoin="round"
    strokeWidth="2"
    viewBox="0 0 24 24"
    width={size}
  >
    <path d="M3 2v6h6" />
    <path d="M3 8a9 9 0 1 0 2.6-4.9L3 6" />
  </svg>
);

export const SplitPreview: FC = () => {
  const { files, setFiles, resetFiles } = usePlayground();
  const [showConfirm, setShowConfirm] = useState(false);
  const confirmTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (confirmTimeoutRef.current) {
        clearTimeout(confirmTimeoutRef.current);
      }
    };
  }, []);

  const handleReset = () => {
    if (showConfirm) {
      if (confirmTimeoutRef.current) {
        clearTimeout(confirmTimeoutRef.current);
        confirmTimeoutRef.current = null;
      }
      resetFiles();
      setShowConfirm(false);
      window.location.reload();
    } else {
      if (confirmTimeoutRef.current) {
        clearTimeout(confirmTimeoutRef.current);
      }
      setShowConfirm(true);
      confirmTimeoutRef.current = setTimeout(() => setShowConfirm(false), 3000);
    }
  };

  return (
    <div className="font-vsc text-vsc-text relative h-full w-full">
      <MonacoEditor
        files={files}
        onFilesChange={setFiles}
        height="100%"
      />
      <button
        onClick={handleReset}
        className="absolute bottom-4 left-4 z-50 flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded bg-vsc-bg-secondary hover:bg-vsc-bg-tertiary border border-vsc-border text-vsc-text-muted hover:text-vsc-text transition-colors"
        title="Reset to default code"
      >
        <ResetIcon size={14} />
        {showConfirm ? 'Click again to confirm' : 'Reset'}
      </button>
    </div>
  );
};
