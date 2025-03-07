import type { Metadata } from 'next';
import { Geist, Geist_Mono } from 'next/font/google';
import './globals.css';
import { ThemeProvider } from '@/components/theme-provider';
import { cn } from '@/lib/utils';
import { Provider as JotaiProvider } from 'jotai';
import { NuqsAdapter } from 'nuqs/adapters/next/app';
import { Toaster } from 'sonner';

const geistSans = Geist({
  variable: '--font-geist-sans',
  subsets: ['latin'],
});

const geistMono = Geist_Mono({
  variable: '--font-geist-mono',
  subsets: ['latin'],
});

export const metadata: Metadata = {
  title: 'BAML Next.js Demo',
  description: 'BAML + Next.js Integration Demo',
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning className="w-full">
      <body
        className={cn(
          'min-h-screen w-full bg-background font-sans antialiased',
          geistSans.variable,
          geistMono.variable,
        )}
      >
        <JotaiProvider>
          <ThemeProvider
            attribute="class"
            defaultTheme="system"
            enableSystem
            disableTransitionOnChange
          >
            <NuqsAdapter>
              <div className="w-full">{children}</div>
            </NuqsAdapter>
            <Toaster />
          </ThemeProvider>
        </JotaiProvider>
      </body>
    </html>
  );
}
