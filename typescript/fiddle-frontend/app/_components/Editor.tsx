'use client'

import CodeMirror, { EditorView, useCodeMirror } from '@uiw/react-codemirror'
import { rust } from '@codemirror/lang-rust'
import { vscodeDark } from '@uiw/codemirror-theme-vscode'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import { useEffect, useRef } from 'react'
import { ASTProvider, FunctionSelector, FunctionPanel, CustomErrorBoundary } from '@baml/playground-common'
import { Button } from '@/components/ui/button'

const extensions = [rust(), EditorView.lineWrapping]
const defaultMainBaml = `
generator lang_python {
  language python
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root ".."
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "pytest -s"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "poetry add baml@latest"
  package_version_command "poetry show baml"
}

function ExtractVerbs {
    input string
    /// list of verbs
    output string[]
}

client<llm> GPT4 {
  provider baml-openai-chat
  options {
    model gpt-4 
    api_key env.OPENAI_API_KEY
  }
}

impl<llm, ExtractVerbs> version1 {
  client GPT4
  prompt #"
    Extract the verbs from this INPUT:
 
    INPUT:
    ---
    {#input}
    ---
    {// this is a comment inside a prompt! //}
    Return a {#print_type(output)}.

    Response:
  "#
}

`

export const Editor = () => {
  return (
    <>
      <ResizablePanelGroup className="min-h-[200px] w-full rounded-lg border" direction="horizontal">
        <ResizablePanel defaultSize={50}>
          <div className="flex w-full h-full">
            <CodeMirror
              value={defaultMainBaml}
              extensions={extensions}
              theme={vscodeDark}
              height="100%"
              width="100%"
              maxWidth="100%"
              style={{ width: '100%', height: '100%' }}
            />
            {/* <div ref={editor} />; */}
          </div>
        </ResizablePanel>
        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={50}>
          <div className="flex items-center justify-center h-full">
            <PlaygroundView />
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </>
  )
}

type LintResponse = {
  diagnostics: LinterError[]
} & (
  | { ok: false }
  | {
      ok: true
      response: any
    }
)

export interface LinterError {
  start: number
  end: number
  text: string
  is_warning: boolean
  source_file: string
}

export interface LinterSourceFile {
  path: string
  content: string
}

export interface LinterInput {
  root_path: string
  files: LinterSourceFile[]
}

const PlaygroundView = () => {
  return (
    <>
      <CustomErrorBoundary>
        <Button
          onClick={async () => {
            const lint = await import('@gloo-ai/baml-schema-wasm-web').then((m) => m.lint)
            const linterInput: LinterInput = {
              root_path: 'baml_src',
              files: [
                {
                  path: 'path/main.baml',
                  content: defaultMainBaml,
                },
              ],
            }
            console.info(`Linting ${linterInput.files.length} files in ${linterInput.root_path}`)
            const res = lint(JSON.stringify(linterInput))
            console.log(`res ${JSON.stringify(res, null, 2)}`)
          }}
        >
          Lint things
        </Button>
        <ASTProvider>
          <div className="absolute z-10 flex flex-col items-end gap-1 right-1 top-2 text-end">
            {/* <TestToggle /> */}
            {/* <VSCodeLink href="https://docs.boundaryml.com">Docs</VSCodeLink> */}
          </div>
          <div className="flex flex-col gap-2 px-2 pb-4">
            <FunctionSelector />
            {/* <Separator className="bg-vscode-textSeparator-foreground" /> */}
            <FunctionPanel />
          </div>
        </ASTProvider>
      </CustomErrorBoundary>
    </>
  )
}

export default Editor
