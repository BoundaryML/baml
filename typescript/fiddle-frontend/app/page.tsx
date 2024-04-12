import Image from 'next/image'
import dynamic from 'next/dynamic'
import { EditorFile, loadUrl } from './actions'
import { BAMLProject, exampleProjects } from '@/lib/exampleProjects'
import { Card, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@baml/playground-common/components/ui/separator'
import { ExampleProjectCard } from './_components/ExampleProjectCard'
const Editor = dynamic(() => import('./_components/Editor'), { ssr: false })

type SearchParams = {
  id: string
}

const defaultMainBaml = `
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
export default async function Home({ searchParams }: { searchParams: SearchParams }) {
  let data: BAMLProject = exampleProjects[0]
  if (searchParams?.id) {
    const exampleProject = exampleProjects.find((p) => p.id === searchParams.id)
    if (exampleProject) {
      data = exampleProject
    } else {
      data = await loadUrl(searchParams.id)
    }
  }
  return (
    <main className="flex flex-col items-center justify-between min-h-screen font-sans">
      <div className="z-10 items-center justify-between w-screen h-screen text-sm overflow-clip lg:flex">
        <div className="w-[200px] justify-start flex flex-col px-1 pr-0 gap-y-2 items-start h-full dark:bg-vscode-sideBar-background">
          <div className="w-full pt-1 text-lg italic font-bold text-center">Prompt Fiddle</div>
          <div className="w-full text-center text-muted-foreground">Examples</div>
          <div className="flex flex-col h-full overflow-y-auto gap-y-2">
            {exampleProjects.map((p) => {
              return <ExampleProjectCard key={p.name} project={p} />
            })}
          </div>
        </div>
        <Separator className="h-full bg-vscode-panel-border" orientation="vertical" />
        <div className="w-screen h-screen dark:bg-black">
          <Editor project={data} />
        </div>
      </div>
    </main>
  )
}
