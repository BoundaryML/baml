'use client';

import dynamic from 'next/dynamic';
import { SplitPreview } from '../playground/SplitPreview';

const DevTools = dynamic(async () => {
  const mod = await import('jotai-devtools');
  return mod.DevTools;
}, { ssr: false });

if (typeof window !== 'undefined' && process.env.NODE_ENV !== 'production') {
  void import('jotai-devtools/styles.css');
}

const Page = () => (
  <div className="flex flex-col h-screen">
    <main className="flex-1 min-h-0">
      <SplitPreview />
    </main>
    {process.env.NODE_ENV !== 'production' ? <DevTools /> : null}
  </div>
);

export default Page;
