import type { Metadata } from 'next'
import { Inter } from 'next/font/google'
import './globals.css'
import { ThemeProvider } from './_components/ThemeProvider'
import JotaiProvider from './_components/JotaiProvider'
import { PHProvider } from './_components/PosthogProvider'
import dynamic from 'next/dynamic'
import { Toaster } from '@/components/ui/toaster'

const PostHogPageView = dynamic(() => import('./PostHogPageView'), {
  ssr: false,
})

const inter = Inter({ subsets: ['latin'] })

export const metadata: Metadata = {
  title: 'Prompt Fiddle',
  description: 'A powerful LLM prompt playground to build, test and share prompts.',
}

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html lang="en">
      <PHProvider>
        <body className={inter.className}>
          <PostHogPageView />
          <ThemeProvider attribute="class" defaultTheme="dark" enableSystem={true} disableTransitionOnChange={true}>
            <Toaster />

            <JotaiProvider>{children}</JotaiProvider>
          </ThemeProvider>
        </body>
      </PHProvider>
    </html>
  )
}
