'use client';

import dynamic from 'next/dynamic';
import Link from 'next/link';
import { SplitPreview } from '@b/pkg-playground';

const DevTools = dynamic(async () => {
  const mod = await import('jotai-devtools');
  return mod.DevTools;
}, { ssr: false });

if (typeof window !== 'undefined' && process.env.NODE_ENV !== 'production') {
  void import('jotai-devtools/styles.css');
}

const Page = () => (
  <main>
    <header>
      <h1>Web Playground</h1>
      <p>
        Shared logic lives in <code>pkg-playground</code>. Head over to the{' '}
        <Link href="https://vitejs.dev/">Vite app</Link> to see it in action there too.
      </p>
    </header>
    <SplitPreview />
    {process.env.NODE_ENV !== 'production' ? <DevTools /> : null}
  </main>
);

export default Page;
