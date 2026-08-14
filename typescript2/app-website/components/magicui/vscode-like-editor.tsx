'use client';

import { useState } from 'react';
import { cn } from '@/lib/utils';
import { SyntaxTypingAnimation } from './syntax-typing-animation';

const FILES = [
  {
    code: `schema Agent {
  input: string
  output: string
}

agent MyAgent implements Agent {
  input: "Analyze the codebase for: {code}"
  output: "Analysis"
}`,
    id: 'agent.baml',
    language: 'yaml',
    name: 'agent.baml',
  },
  {
    code: `function AnalyzeCodebase(code: string): Analysis {
  // Uses OpenAI GPT-4o
  return callOpenAI({ prompt: \`Analyze the codebase for: \` });
}

const result = AnalyzeCodebase('Find all auth endpoints');
console.log(result);`,
    id: 'main.ts',
    language: 'typescript',
    name: 'main.ts',
  },
  {
    code: `def analyze_codebase(code: str) -> dict:
    # Uses OpenAI GPT-4o
    prompt = f"Analyze the codebase for: {code}"
    return call_openai(prompt)

result = analyze_codebase("Find all auth endpoints")
print(result)`,
    id: 'main.py',
    language: 'python',
    name: 'main.py',
  },
  {
    code: `def analyze_codebase(code)
  # Uses OpenAI GPT-4o
  prompt = "Analyze the codebase for: #{code}"
  call_openai(prompt)
end

result = analyze_codebase('Find all auth endpoints')
puts result
`,
    id: 'main.rb',
    language: 'ruby',
    name: 'main.rb',
  },
];

export function VscodeLikeEditor({ className }: { className?: string }) {
  const [selectedFile, setSelectedFile] = useState(FILES[0]);

  return (
    <div
      className={cn(
        'flex w-full h-[400px] max-w-4xl rounded-xl border border-border bg-background overflow-hidden shadow-lg',
        'md:h-[500px]',
        className,
      )}
    >
      {/* Sidebar */}
      <aside className="hidden md:flex flex-col w-48 bg-muted border-r border-border h-full">
        <div className="flex items-center h-10 px-4 font-semibold text-xs tracking-widest border-b border-border uppercase text-muted-foreground">
          Explorer
        </div>
        <div className="flex-1 py-2 pr-2">
          {FILES.map((file) => (
            <button
              className={cn(
                'flex items-center w-full px-3 py-1.5 text-sm rounded hover:bg-accent transition',
                selectedFile.id === file.id &&
                  'bg-accent text-accent-foreground font-semibold',
              )}
              key={file.id}
              onClick={() => setSelectedFile(file)}
              type="button"
            >
              {file.name}
            </button>
          ))}
        </div>
      </aside>
      {/* Mobile sidebar (top) */}
      <div className="flex md:hidden w-full border-b border-border bg-muted overflow-x-auto">
        {FILES.map((file) => (
          <button
            className={cn(
              'px-4 py-2 text-sm whitespace-nowrap',
              selectedFile.id === file.id &&
                'bg-accent text-accent-foreground font-semibold',
            )}
            key={file.id}
            onClick={() => setSelectedFile(file)}
            type="button"
          >
            {file.name}
          </button>
        ))}
      </div>
      {/* Editor */}
      <main className="flex-1 flex flex-col bg-background h-full">
        <div className="flex items-center h-10 px-4 border-b border-border text-xs text-muted-foreground bg-background">
          {selectedFile.name}
        </div>
        <div className="flex-1 overflow-auto p-4">
          <SyntaxTypingAnimation
            className="min-h-[200px] text-[13px]"
            code={selectedFile.code}
            delay={200}
            duration={30}
            language={selectedFile.language}
          />
        </div>
      </main>
    </div>
  );
}
