import { useState } from 'react'
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '../components/ui/resizable'
import { TooltipProvider } from '../components/ui/tooltip'
import { Button } from '../components/ui/button'

const CodeGridPreview: React.FC = () => {
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null)

  const codeSnippets = [
    {
      code: `client<llm> GPT4Turbo {
      provider openai
      options {
        model gpt-4-turbo
        api_key env.OPENAI_API_KEY
      }
    } `,
      alt: 'Building a chatbot',
      fileUrl: 'https://example.com/code/client.llm',
    },
    { code: `const b = 2;`, alt: 'Code 2', fileUrl: 'https://example.com/code/code2.js' },
    { code: `const c = 3;`, alt: 'Code 3', fileUrl: 'https://example.com/code/code3.js' },
    { code: `const d = 4;`, alt: 'Code 4', fileUrl: 'https://example.com/code/code4.js' },
    { code: `const e = 5;`, alt: 'Code 5', fileUrl: 'https://example.com/code/code5.js' },
    { code: `const f = 6;`, alt: 'Code 6', fileUrl: 'https://example.com/code/code6.js' },
    { code: `const g = 7;`, alt: 'Code 7', fileUrl: 'https://example.com/code/code7.js' },
    { code: `const h = 8;`, alt: 'Code 8', fileUrl: 'https://example.com/code/code8.js' },
  ]

  return (
    <div>
      {' '}
      <div className='relative flex justify-center items-center mx-auto gap-2 text-lg font-semibold text-white'>
        Basics of BAML
      </div>
      <div className='grid gap-4 p-4' style={{ gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))' }}>
        {codeSnippets.map((snippet, idx) => (
          <div key={idx} className='flex flex-col items-center p-2'>
            <div className='w-full p-2 mt-2 text-center text-white'>{snippet.alt}</div>

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
              ></div>
            </div>
          </div>
        ))}
      </div>
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

      <Button
        className='align-middle self-center mt-4'
        onClick={() => window.open('https://docs.boundaryml.com', '_blank', 'noopener,noreferrer')}
      >
        See full docs here!
      </Button>
    </div>
  )
}

export default LibraryGrid
