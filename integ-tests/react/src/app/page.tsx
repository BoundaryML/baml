import { ModeToggle } from '@/components/theme-toggle'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { GithubIcon } from 'lucide-react'
import Image from 'next/image'
import TestClient from '../components/hook-example'

export default function Home() {
  return (
    <div className='min-h-screen bg-background'>
      <div className='container mx-auto px-4 py-8'>
        <header className='flex justify-between items-center mb-8'>
          <div className='flex items-center gap-2'>
            <Image className='dark:invert' src='/next.svg' alt='Next.js logo' width={100} height={20} priority />
            <span className='text-lg font-mono'>+</span>
            <span className='text-lg font-bold'>BAML</span>
          </div>
          <div className='flex items-center gap-4'>
            <Button asChild variant='outline'>
              <a href='https://docs.boundaryml.com' target='_blank' rel='noopener noreferrer'>
                Documentation
              </a>
            </Button>
            <Button asChild>
              <a href='https://docs.boundaryml.com/docs/examples' target='_blank' rel='noopener noreferrer'>
                View Examples
              </a>
            </Button>
            <Button variant='outline' size='icon' asChild>
              <a href='https://github.com/boundaryml/baml' target='_blank' rel='noopener noreferrer'>
                <GithubIcon className='h-4 w-4' />
              </a>
            </Button>
            <ModeToggle />
          </div>
        </header>

        <main className='max-w-4xl mx-auto space-y-8'>
          <div className='text-center space-y-4'>
            <h1 className='text-4xl font-bold tracking-tight'>BAML + Next.js Integration</h1>
            <p className='text-lg text-muted-foreground'>Select an example below to get started.</p>
            <div className='w-[200px] mx-auto'>
              <Select defaultValue='chat'>
                <SelectTrigger>
                  <SelectValue placeholder='Select an example' />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value='chat'>Chat Interface</SelectItem>
                  <SelectItem value='classification'>Text Classification</SelectItem>
                  <SelectItem value='extraction'>Data Extraction</SelectItem>
                  <SelectItem value='summarization'>Text Summarization</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <div className='flex justify-center gap-4 max-w-xl mx-auto'>
            <TestClient />
          </div>
        </main>

        <footer className='mt-16 text-center'>
          <p className='text-sm text-muted-foreground'>
            Built with{' '}
            <a
              href='https://nextjs.org'
              target='_blank'
              rel='noopener noreferrer'
              className='font-medium underline underline-offset-4'
            >
              Next.js
            </a>{' '}
            and{' '}
            <a
              href='https://boundaryml.com'
              target='_blank'
              rel='noopener noreferrer'
              className='font-medium underline underline-offset-4'
            >
              BAML
            </a>
          </p>
        </footer>
      </div>
    </div>
  )
}
