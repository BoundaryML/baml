'use client';

import { useAtomValue } from 'jotai';
import { useWasmReady, useUpdateFile } from './BamlPlaygroundProvider';
import { filesAtom } from './atoms';

interface BamlEditorProps {
  /** The file path to edit */
  filePath: string;
  /** Custom class name */
  className?: string;
  /** Custom style */
  style?: React.CSSProperties;
}

/**
 * A textarea editor for BAML source code.
 * Automatically syncs changes with the WASM project.
 */
export function BamlEditor({ filePath, className, style }: BamlEditorProps) {
  const isReady = useWasmReady();
  const files = useAtomValue(filesAtom);
  const updateFile = useUpdateFile();

  const content = files[filePath] ?? '';

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    updateFile(filePath, e.target.value);
  };

  return (
    <textarea
      value={content}
      onChange={handleChange}
      disabled={!isReady}
      className={className}
      spellCheck={false}
      style={{
        width: '100%',
        height: '100%',
        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        fontSize: '14px',
        lineHeight: '1.6',
        padding: '16px',
        border: 'none',
        outline: 'none',
        resize: 'none',
        backgroundColor: '#0a0a0a',
        color: '#ededed',
        ...style,
      }}
      placeholder={isReady ? 'Enter BAML code...' : 'Loading WASM...'}
    />
  );
}
