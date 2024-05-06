import type { Metadata } from 'next'
import { Inter } from 'next/font/google'
import './globals.css'
import { ThemeProvider } from './_components/ThemeProvider'
import JotaiProvider from './_components/JotaiProvider'
import { PHProvider } from './_components/PosthogProvider'
import dynamic from 'next/dynamic'
import { Toaster } from '@/components/ui/toaster'
import { BrowseSheet } from './_components/BrowseSheet'
import { Suspense } from 'react'

const PostHogPageView = dynamic(() => import('./PostHogPageView'), {
  ssr: false,
})

const inter = Inter({ subsets: ['latin'] })

export const metadata: Metadata = {
  title: 'Prompt Fiddle',
  description: 'An LLM prompt playground for structured prompting',
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html lang='en'>
      <PHProvider>
        <body className={inter.className}>
          <PostHogPageView />
          <ThemeProvider attribute='class' defaultTheme='dark' enableSystem={false} disableTransitionOnChange={true}>
            <Toaster />

            <JotaiProvider>
              <>
                <Suspense fallback={<div>Loading...</div>}>{children}</Suspense>
                <div className='fixed left-0 bottom-1/2 w-[12%] px-1 items-center justify-center flex'>
                  <BrowseSheet />
                </div>
              </>
            </JotaiProvider>
          </ThemeProvider>
        </body>
      </PHProvider>
    </html>
  )
}
