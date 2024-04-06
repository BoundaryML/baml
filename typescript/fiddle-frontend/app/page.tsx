import Image from 'next/image'
import dynamic from 'next/dynamic'

const Editor = dynamic(() => import('./_components/Editor'), { ssr: false })

export default function Home() {
  return (
    <main className="flex flex-col items-center justify-between min-h-screen">
      <div className="z-10 items-center justify-between w-screen h-screen font-mono text-sm overflow-clip lg:flex">
        <Editor />
      </div>
    </main>
  )
}
