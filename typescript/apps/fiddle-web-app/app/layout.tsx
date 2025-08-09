import '@baml/ui/globals.css';
import { Toaster } from '@baml/ui/sonner';
import type { Metadata, Viewport } from 'next';
import { Inter } from 'next/font/google';
import { Suspense } from 'react';
import { ErrorBoundary } from 'react-error-boundary';
import { PHProvider, RB2BElement } from './_components/PosthogProvider';
import { ThemeProvider } from './_components/ThemeProvider';
import { cn } from '@baml/ui/lib/utils';
import PostHogPageView from './PostHogPageView';

import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';

const inter = Inter({ subsets: ['latin'] });

export const metadata: Metadata = {
  title: 'Prompt Fiddle',
  description: 'An LLM prompt playground for structured prompting',
};

export const viewport: Viewport = {
  themeColor: [
    { color: 'white', media: '(prefers-color-scheme: light)' },
    { color: 'black', media: '(prefers-color-scheme: dark)' },
  ],
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <RB2BElement />
      <PHProvider>
        <body
          className={cn(
            'bg-background text-foreground relative min-h-screen font-sans antialiased',
            GeistSans.variable,
            GeistMono.variable,
          )}
        >
          <ErrorBoundary fallback={null}>
            <PostHogPageView />
          </ErrorBoundary>
          <ThemeProvider
            attribute="class"
            defaultTheme="dark"
            enableSystem={false}
            disableTransitionOnChange={true}
          >
            {/* <JotaiProvider> */}
            <Suspense fallback={null}>{children}</Suspense>
            {/* <div className="fixed left-0 bottom-1/2 w-[12%] px-1 items-center justify-center flex">
                <BrowseSheet />
              </div> */}
            {/* <PromptPreview /> */}
            {/* </JotaiProvider> */}
            <Toaster />
          </ThemeProvider>
        </body>
      </PHProvider>
    </html>
  );
}
