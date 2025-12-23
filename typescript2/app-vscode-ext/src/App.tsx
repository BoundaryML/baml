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

class Person {
  name string
  age int
}
`,
  'baml_src/translate.baml': `
function Translate(text: string, targetLang: string) -> string {
  client GPT4
  prompt "Translate the text"
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
    <div style={{ display: 'grid', gridTemplateColumns: '250px 1fr', gap: '20px', height: '100vh' }}>
      {/* Sidebar */}
      <div style={{ background: '#252526', padding: '16px', borderRadius: '8px' }}>
        <h2 style={{ fontSize: '14px', marginBottom: '12px', color: '#cccccc' }}>
          Functions ({functions.length})
        </h2>
        <FunctionList
          onSelect={setSelected}
          selected={selected}
        />
      </div>

      {/* Main content */}
      <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
        <div style={{ background: '#252526', padding: '16px', borderRadius: '8px' }}>
          <h1 style={{ fontSize: '20px', marginBottom: '8px' }}>BAML Playground v2</h1>
          <p style={{ color: '#888' }}>
            {isReady ? '✅ WASM loaded' : error ? `❌ ${error}` : '⏳ Loading...'}
          </p>
          {selected && (
            <p style={{ marginTop: '8px', color: '#4fc1ff' }}>
              Selected: <code>{selected}</code>
            </p>
          )}
        </div>

        {/* Editor */}
        <div style={{ flex: 1, background: '#1e1e1e', borderRadius: '8px', overflow: 'hidden' }}>
          <div style={{ padding: '8px 16px', background: '#252526', borderBottom: '1px solid #3c3c3c' }}>
            <span style={{ fontSize: '12px', color: '#888' }}>baml_src/main.baml</span>
          </div>
          <textarea
            value={editorContent}
            onChange={handleEditorChange}
            style={{
              width: '100%',
              height: 'calc(100% - 40px)',
              background: '#1e1e1e',
              color: '#d4d4d4',
              border: 'none',
              padding: '16px',
              fontFamily: 'Menlo, Monaco, "Courier New", monospace',
              fontSize: '13px',
              lineHeight: '1.5',
              resize: 'none',
              outline: 'none',
            }}
            spellCheck={false}
          />
        </div>
      </div>
    </div>
  );
}

export default function App() {
  return (
    <BamlPlaygroundProvider initialFiles={SAMPLE_FILES} rootDir="baml_src">
      <PlaygroundContent />
    </BamlPlaygroundProvider>
  );
}
