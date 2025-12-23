import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'BAML Playground v2 - PromptFiddle',
  description: 'Build and test BAML functions',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body
        style={{
          margin: 0,
          fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
          background: '#0a0a0a',
          color: '#ededed',
          minHeight: '100vh',
        }}
      >
        {children}
      </body>
    </html>
  );
}
