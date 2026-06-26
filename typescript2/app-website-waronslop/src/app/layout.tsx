import type { Metadata } from 'next';
import { ReactNode } from 'react';
import ConvexClientProvider from '@/components/ConvexClientProvider';
import './globals.css';

export const metadata: Metadata = {
  title: 'fight slop with slop',
  description:
    'A standing legion against the great unwashed tide of AI slop. Order, craft, and the hand-made — marching east to meet the Slopmonsters.',
  metadataBase: new URL('https://waronslop.com'),
  openGraph: {
    title: 'The War on Slop',
    description: 'Order, craft, and the hand-made versus the great unwashed tide.',
    url: 'https://waronslop.com',
    siteName: 'The War on Slop',
    type: 'website',
  },
};

const RootLayout = ({ children }: { children: ReactNode }) => (
  <html lang="en">
    <body>
      <ConvexClientProvider>{children}</ConvexClientProvider>
    </body>
  </html>
);

export default RootLayout;
