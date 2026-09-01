import type { Metadata } from 'next';
import { RootProvider } from 'fumadocs-ui/provider/next';
import { siteDescription, siteName, siteUrl } from '@/lib/constants';
import './global.css';

export const metadata: Metadata = {
  metadataBase: new URL(siteUrl),
  title: {
    default: siteName,
    template: `%s | ${siteName}`,
  },
  description: siteDescription,
  robots:
    process.env.VERCEL_ENV === 'production'
      ? { index: true, follow: true }
      : { index: false, follow: false },
};

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className="flex min-h-screen flex-col">
        <RootProvider>{children}</RootProvider>
      </body>
    </html>
  );
}
