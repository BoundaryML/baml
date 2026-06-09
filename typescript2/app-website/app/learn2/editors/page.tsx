'use client';

import { BamlEditor } from '../_editor/BamlEditor';
import '../learn2.css';

// Each snippet becomes its own isolated project in the shared runtime.
const SNIPPETS: { title: string; code: string }[] = [
  {
    title: 'Hello, BAML',
    code: `function greet(name: string) -> string {
  let greeting = "hello, " + name
  greeting + "!"
}

function shout(name: string) -> string {
  let loud = greet(name)
  loud.to_upper_case()
}

test "greets" {
  assert.equal(greet("sheep"), "hello, sheep!")
}`,
  },
  {
    title: 'Classes & methods',
    code: `class Greeting {
  message: string,

  function shout(self) -> string {
    let loud = self.message.to_upper_case()
    loud
  }
}`,
  },
  {
    title: 'Unions & types',
    code: `type Label = "positive" | "negative" | "neutral";

class Verdict {
  label: Label,
  confidence: float,
}`,
  },
  {
    title: 'Testsets',
    code: `testset "math" {
  test "adds" {
    assert.equal(1 + 1, 2)
  }
  test "subtracts" {
    assert.equal(2 - 1, 1)
  }
}`,
  },
  {
    title: 'Try breaking it',
    code: `function add(a: int, b: int) -> int {
  let sum = a + b
  sum
}

test "adds" {
  assert.equal(add(2, 2), 4)
}`,
  },
];

export default function EditorsDemoPage() {
  return (
    <div className="l2-editors-demo">
      <header className="l2-editors-head">
        <a href="/learn2" className="font-mono">
          ← deck
        </a>
        <span className="font-mono">BAML editors · scroll demo</span>
      </header>
      <main className="l2-editors-main">
        <p className="l2-editors-intro">
          Many live editors on one page — each its own project, sharing one
          runtime. Edit any of them; diagnostics are per-editor (try the last
          one: rename <code>add</code> or delete a paren).
        </p>
        {SNIPPETS.map((s) => (
          <section className="l2-editors-cell" key={s.title}>
            <h2 className="l2-editors-cell-title font-mono">{s.title}</h2>
            <BamlEditor initialCode={s.code} height={200} />
          </section>
        ))}
      </main>
    </div>
  );
}
