'use client';

import clsx from 'clsx';
import type React from 'react';
import { useEffect, useState } from 'react';

type FileData = {
  code: string;
  filename: string;
  language: string;
};

type VSCodeMockProps = {
  // Support both single file (backward compatibility) and multiple files
  code?: string;
  filename?: string;
  language?: string;
  files?: FileData[];
  dark?: boolean;
  showSidebar?: boolean;
  showStatusBar?: boolean;
  lineNumbers?: boolean;
  height?: number | string;
  className?: string;
  showTerminal?: boolean;
  terminalHeight?: number;
  terminalContent?: React.ReactNode;
};

export const VSCodeMock: React.FC<VSCodeMockProps> = ({
  code,
  filename = 'index.ts',
  language = 'TypeScript',
  files,
  dark = true,
  showSidebar = true,
  showStatusBar = true,
  lineNumbers = true,
  height = 420,
  className,
  showTerminal = false,
  terminalHeight = 120,
  terminalContent,
}) => {
  // Determine which files to use
  const fileList = files || [{ code: code || '', filename, language }];
  const [activeFileIndex, setActiveFileIndex] = useState(0);
  const activeFile = fileList[activeFileIndex];

  const lines = activeFile.code.split('\n');

  const bg = dark ? 'bg-[#1e1e1e]' : 'bg-white';
  const fg = dark ? 'text-[#d4d4d4]' : 'text-[#111827]';
  const subtle = dark ? 'text-[#6b7280]' : 'text-[#9ca3af]';
  const border = dark ? 'border-[#2b2b2b]' : 'border-gray-200';
  const tabBg = dark ? 'bg-[#252526]' : 'bg-gray-100';
  const statusBg = dark ? 'bg-[#007acc]' : 'bg-sky-500';
  const gutter = dark ? 'bg-[#252526]' : 'bg-gray-100';
  // const terminalBg = dark ? 'bg-[#1e1e1e]' : 'bg-gray-900';
  // const terminalFg = dark ? 'text-[#d4d4d4]' : 'text-gray-100';

  return (
    <div
      className={clsx(
        'rounded-xl shadow-2xl overflow-hidden border',
        border,
        className,
      )}
      // style={{ height }}
    >
      {/* Title bar */}
      <div
        className={clsx(
          'flex items-center h-8 px-3 border-b',
          border,
          dark ? 'bg-[#2d2d2d]' : 'bg-gray-100',
        )}
      >
        {/* macOS traffic lights */}
        <div className="flex items-center gap-2 mr-3">
          <span className="w-3 h-3 rounded-full bg-[#ff5f57]" />
          <span className="w-3 h-3 rounded-full bg-[#ffbd2f]" />
          <span className="w-3 h-3 rounded-full bg-[#28c940]" />
        </div>
        <span className={clsx('text-xs', subtle)}>
          {activeFile.filename} — {activeFile.language} — Visual Studio Code
        </span>
      </div>

      {/* Tabs */}
      <div
        className={clsx(
          'flex items-center h-9 text-sm',
          tabBg,
          fg,
          'border-b',
          border,
        )}
      >
        {fileList.map((file, index) => (
          <button
            className={clsx(
              'px-3 h-full flex items-center border-r',
              border,
              index === activeFileIndex
                ? dark
                  ? 'bg-[#1e1e1e]'
                  : 'bg-white'
                : 'bg-transparent hover:bg-white/10',
              'transition-colors',
            )}
            key={file.filename}
            onClick={() => setActiveFileIndex(index)}
            type="button"
          >
            {file.filename}
          </button>
        ))}
      </div>

      {/* Main area */}
      <div
        className={clsx('flex w-full', bg, fg)}
        // style={{
        //   height: `calc(${typeof height === 'number' ? `${height}px` : height} - ${
        //     64 + (showStatusBar ? 32 : 0) + (showTerminal ? terminalHeight : 0)
        //   }px)`,
        // }}
      >
        {/* Sidebar */}
        {showSidebar && (
          <div
            className={clsx(
              'w-14 border-r',
              border,
              'flex flex-col items-center py-3 gap-4',
              dark ? 'bg-[#252526]' : 'bg-gray-50',
            )}
          >
            {/* Fake sidebar icons */}
            <VSCodeIcon className="codicon-files" dot />
            <VSCodeIcon className="codicon-search" />
            <VSCodeIcon className="codicon-source-control" />
            <VSCodeIcon className="codicon-debug-alt-small" />
            <VSCodeIcon className="codicon-extensions" />
          </div>
        )}

        {/* Editor */}
        <div className="flex-1 flex overflow-hidden text-sm leading-6">
          {lineNumbers && (
            <div
              className={clsx(
                'select-none px-3 py-2 text-right',
                subtle,
                'border-r',
                border,
                gutter,
                'whitespace-pre',
              )}
            >
              {lines.map((_, i) => (
                <div key={`line-${i + 1}`}>{i + 1}</div>
              ))}
            </div>
          )}
          <div className="flex-1 overflow-auto px-4 py-2">
            <pre className="whitespace-pre">
              <code>{activeFile.code}</code>
            </pre>
          </div>
        </div>
      </div>

      {/* Terminal */}
      {showTerminal && false && (
        <div
          className={clsx('border-t', border)}
          style={{ height: terminalHeight }}
        >
          {/* Terminal header */}
          <div
            className={clsx(
              'flex items-center justify-between h-8 px-3',
              tabBg,
              'text-xs',
              fg,
            )}
          >
            <div className="flex items-center gap-2">
              <span className={fg}>TERMINAL</span>
              <span className={subtle}>bash</span>
            </div>
            <div className="flex items-center gap-2">
              <button
                aria-label="Clear terminal"
                className="hover:bg-white/10 p-1 rounded"
                type="button"
              >
                <VSCodeIcon className="codicon-trash text-sm" />
              </button>
              <button
                aria-label="Close terminal"
                className="hover:bg-white/10 p-1 rounded"
                type="button"
              >
                <VSCodeIcon className="codicon-close text-sm" />
              </button>
            </div>
          </div>
          {/* Terminal content */}
          <div
            className={clsx(
              'p-3 overflow-auto font-mono text-sm',
              dark ? 'bg-[#1e1e1e]' : 'bg-white',
            )}
            // style={{ height: terminalHeight - 32 }}
          >
            {terminalContent || (
              <div className="flex items-center">
                <span className="text-primary mr-2">$</span>
                <span className={subtle}>Type a command...</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Status bar */}
      {showStatusBar && (
        <div
          className={clsx(
            'h-8 px-3 flex items-center justify-between text-xs text-white',
            statusBg,
          )}
        >
          <div className="flex items-center gap-3">
            <span>● {activeFile.language}</span>
            <span>LF</span>
            <span>UTF-8</span>
          </div>
          <div className="flex items-center gap-3">
            <span>Ln {lines.length}, Col 1</span>
          </div>
        </div>
      )}
    </div>
  );
};

const VSCodeIcon: React.FC<{ className?: string; dot?: boolean }> = ({
  className,
  dot,
}) => (
  <div className="relative">
    <span className={clsx('codicon', className, 'text-xl opacity-80')} />
    {dot && (
      <span className="absolute -top-1 -right-1 w-2 h-2 bg-blue-500 rounded-full" />
    )}
  </div>
);
