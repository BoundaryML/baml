import type { Metadata } from 'next';
import { Suspense } from 'react';
import './learn2/learn2.css';
import './learn3/learn3.css';
import './learn4/learn4.css';
import './baml-intro/baml-intro.css';
import { Article } from './baml-intro/_components/Article';

// Homepage currently mirrors /baml-intro. The previous homepage is preserved
// at app/home-old/page.tsx (reachable at /home-old).
export const metadata: Metadata = {
  description:
    'Statically typed like Rust, flexible like TypeScript, parallel like Go. Fully qualified names, no imports, native tests, baml describe, baml pack — every feature built so agents make fewer mistakes.',
  title: 'BAML — the programming language for agents',
};

export default function Page() {
  return (
    <Suspense>
      <Article view="intro" />
    </Suspense>
  );
}
