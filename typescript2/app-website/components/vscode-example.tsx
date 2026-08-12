'use client';

import { TypingAnimation } from '@/components/magicui/terminal';
import { VSCodeMock } from '@/components/vscode';

const codeExample = `function analyzeCode(filePath: string) {
  const ast = parseAST(filePath);
  const issues = findIssues(ast);

  return {
    totalLines: ast.lineCount,
    issues: issues.length,
    complexity: calculateComplexity(ast)
  };
}

// Run the analysis
const results = analyzeCode("./src/index.ts");
console.log(results);`;

const terminalOutput = `$ npm run analyze
> code-analyzer@1.0.0 analyze
> node ./scripts/analyze.js

Analyzing codebase...
✓ Parsed 42 files
✓ Found 3 potential issues
✓ Generated report

Results:
- Total lines: 1,284
- Issues found: 3
- Average complexity: 4.2

Analysis complete! Report saved to ./reports/analysis.json`;

export function VSCodeExample() {
  return (
    <div className="w-full max-w-4xl mx-auto p-6">
      <h2 className="text-2xl font-bold mb-4">VS Code with Terminal Demo</h2>

      <VSCodeMock
        code={codeExample}
        dark={true}
        filename="analyzer.ts"
        height={600}
        language="typescript"
        showTerminal={true}
        terminalContent={
          <TypingAnimation
            className="text-green-400"
            delay={1000}
            duration={30}
          >
            {terminalOutput}
          </TypingAnimation>
        }
        terminalHeight={200}
      />
    </div>
  );
}
