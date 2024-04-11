import Image from 'next/image'
import dynamic from 'next/dynamic'
import { EditorFile, loadUrl } from './actions'
import { BAMLProject, exampleProjects } from '@/lib/exampleProjects'
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
    data = await loadUrl(searchParams.id)
  }
  console.log('loaded data ', data)
  return (
    <main className="flex flex-col items-center justify-between min-h-screen">
      <div className="z-10 items-center justify-between w-screen h-screen font-mono text-sm overflow-clip lg:flex">
        <Editor project={data} />
      </div>
    </main>
  )
}
