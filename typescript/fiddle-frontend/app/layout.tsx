import type { Metadata } from 'next'
import { Inter } from 'next/font/google'
import './globals.css'
import { Toaster } from '@/components/ui/toaster'
import JotaiProvider from '@baml/playground-common/baml_wasm_web/JotaiProvider'
import dynamic from 'next/dynamic'
import { Suspense } from 'react'
import { BrowseSheet } from './_components/BrowseSheet'
import { PHProvider, RB2BElement } from './_components/PosthogProvider'
import { ThemeProvider } from './_components/ThemeProvider'

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
      <RB2BElement />
      <PHProvider>
        <body className={inter.className}>
          <PostHogPageView />
          <ThemeProvider attribute='class' defaultTheme='dark' enableSystem={false} disableTransitionOnChange={true}>
            <JotaiProvider>
              <Suspense fallback={<div>Loading...</div>}>{children}</Suspense>
              <div className='fixed left-0 bottom-1/2 w-[12%] px-1 items-center justify-center flex'>
                <BrowseSheet />
              </div>
            </JotaiProvider>
            <Toaster />
          </ThemeProvider>
        </body>
      </PHProvider>
    </html>
  )
}
