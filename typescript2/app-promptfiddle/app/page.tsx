'use client';

import { useState } from 'react';
import {
  BamlPlaygroundProvider,
  FunctionList,
  useBamlPlayground,
} from '@baml/playground-common';

// Sample BAML files for testing
const SAMPLE_FILES = {
  'baml_src/main.baml': `
function ExtractName(text: string) -> string {
  client GPT4
  prompt "Extract the person's name from the text"
}

function Summarize(document: string) -> string {
  client GPT4
  prompt "Summarize the document"
}

function Classify(text: string) -> string {
  client GPT4
  prompt "Classify the sentiment"
}
`,
};

function PlaygroundContent() {
  const { isReady, error, functions, updateFile } = useBamlPlayground();
  const [selected, setSelected] = useState<string | undefined>();
  const [editorContent, setEditorContent] = useState(SAMPLE_FILES['baml_src/main.baml']);

  const handleEditorChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newContent = e.target.value;
    setEditorContent(newContent);
    updateFile('baml_src/main.baml', newContent);
  };

  return (
    <div style={{ display: 'flex', minHeight: '100vh' }}>
      {/* Sidebar */}
      <aside
        style={{
          width: '280px',
          background: '#171717',
          borderRight: '1px solid #262626',
          padding: '24px',
        }}
      >
        <h1 style={{ fontSize: '18px', fontWeight: 600, marginBottom: '24px' }}>
          PromptFiddle v2
        </h1>

        <div style={{ marginBottom: '16px' }}>
          <span
            style={{
              fontSize: '12px',
              color: isReady ? '#22c55e' : error ? '#ef4444' : '#a1a1aa',
            }}
          >
            {isReady ? '● WASM Ready' : error ? `● Error: ${error}` : '○ Loading...'}
          </span>
        </div>

        <h2 style={{ fontSize: '12px', color: '#a1a1aa', marginBottom: '12px' }}>
          FUNCTIONS ({functions.length})
        </h2>

        <FunctionList onSelect={setSelected} selected={selected} />
      </aside>

      {/* Main content */}
      <main style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
        {/* Header */}
        <header
          style={{
            padding: '16px 24px',
            borderBottom: '1px solid #262626',
            display: 'flex',
            alignItems: 'center',
            gap: '16px',
          }}
        >
          <span style={{ fontSize: '14px', color: '#a1a1aa' }}>baml_src/main.baml</span>
          {selected && (
            <span style={{ fontSize: '14px', color: '#3b82f6' }}>
              → {selected}
            </span>
          )}
        </header>

        {/* Editor */}
        <div style={{ flex: 1, position: 'relative' }}>
          <textarea
            value={editorContent}
            onChange={handleEditorChange}
            style={{
              position: 'absolute',
              inset: 0,
              width: '100%',
              height: '100%',
              background: '#0a0a0a',
              color: '#ededed',
              border: 'none',
              padding: '24px',
              fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
              fontSize: '14px',
              lineHeight: '1.6',
              resize: 'none',
              outline: 'none',
            }}
            spellCheck={false}
          />
        </div>
      </main>
    </div>
  );
}

export default function Home() {
  return (
    <BamlPlaygroundProvider initialFiles={SAMPLE_FILES} rootDir="baml_src">
      <PlaygroundContent />
    </BamlPlaygroundProvider>
  );
}
