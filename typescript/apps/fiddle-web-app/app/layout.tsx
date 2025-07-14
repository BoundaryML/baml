import { Toaster } from '@baml/ui/sonner';
import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import { Suspense } from 'react';
import { ErrorBoundary } from 'react-error-boundary';
import { PHProvider, RB2BElement } from './_components/PosthogProvider';
import { ThemeProvider } from './_components/ThemeProvider';
import '@baml/ui/globals.css';
import PostHogPageView from './PostHogPageView';

const inter = Inter({ subsets: ['latin'] });

export const metadata: Metadata = {
  title: 'Prompt Fiddle',
  description: 'An LLM prompt playground for structured prompting',
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <RB2BElement />
      <PHProvider>
        <body className={'bg-background'}>
          <ErrorBoundary fallback={<div></div>}>
            <PostHogPageView />
          </ErrorBoundary>
          <ThemeProvider
            attribute="class"
            defaultTheme="dark"
            enableSystem={false}
            disableTransitionOnChange={true}
          >
            {/* <JotaiProvider> */}
            <Suspense fallback={<div>Loading...</div>}>{children}</Suspense>
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
