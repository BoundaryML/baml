import { useState } from 'react'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'
import { TooltipProvider } from '../components/ui/tooltip'

const CodeGridPreview: React.FC = () => {
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null)

  const codeSnippets = [
    { code: `const a = 1;`, alt: 'Code 1' },
    { code: `const b = 2;`, alt: 'Code 2' },
    { code: `const c = 3;`, alt: 'Code 3' },
    { code: `const d = 4;`, alt: 'Code 4' },
    { code: `const e = 5;`, alt: 'Code 5' },
    { code: `const f = 6;`, alt: 'Code 6' },
    { code: `const g = 7;`, alt: 'Code 7' },
    { code: `const h = 8;`, alt: 'Code 8' },
  ]

  return (
    <div className='grid gap-4 p-4' style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))' }}>
      {codeSnippets.map((snippet, idx) => (
        <div key={idx} className='flex flex-col items-center p-2'>
          <div
            className='relative w-full cursor-pointer'
            style={{ paddingBottom: '150%' }} // Maintain aspect ratio 2:3
            onClick={() => setExpandedIndex(idx === expandedIndex ? null : idx)}
          >
            <div
              className={`absolute top-0 left-0 w-full h-full p-2 overflow-auto border rounded-md bg-black text-white ${
                idx === expandedIndex ? 'z-50' : ''
              }`}
              style={
                idx === expandedIndex
                  ? {
                      position: 'fixed',
                      top: '10%',
                      left: '10%',
                      right: '10%',
                      bottom: '10%',
                      maxHeight: '80vh',
                      maxWidth: '80vw',
                    }
                  : {}
              }
            >
              <pre className='whitespace-pre-wrap'>{snippet.code}</pre>
            </div>
          </div>
          <div className='w-full p-2 mt-2 text-center text-white'>{snippet.alt}</div>
        </div>
      ))}
    </div>
  )
}

const LibraryGrid: React.FC = () => {
  return (
    <div
      className='flex flex-col w-full overflow-auto'
      style={{
        height: 'calc(100vh - 80px)',
        minWidth: '300px', // Ensure the minimum width allows for responsive design
      }}
    >
      <TooltipProvider>
        <ResizablePanelGroup direction='vertical' className='h-full'>
          <ResizablePanel id='top-panel' className='flex w-full px-1' defaultSize={50}>
            <div className='w-full'>
              <ResizablePanelGroup direction='horizontal' className='h-full'>
                <div className='relative w-full h-full overflow-y-auto'>
                  <CodeGridPreview />
                </div>
              </ResizablePanelGroup>
            </div>
          </ResizablePanel>
        </ResizablePanelGroup>
      </TooltipProvider>
    </div>
  )
}

export default LibraryGrid
