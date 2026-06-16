import type { Metadata } from 'next';
import { Suspense } from 'react';
import '../learn2/learn2.css';
import '../learn3/learn3.css';
import '../learn4/learn4.css';
import './baml-intro.css';
import { Article } from './_components/Article';

export const metadata: Metadata = {
  description:
    'Statically typed like Rust, flexible like TypeScript, parallel like Go. Fully qualified names, no imports, native tests, baml describe, baml pack — every feature built so agents make fewer mistakes.',
  title: 'BAML — the programming language for agents',
};

export default function BamlIntroPage() {
  // Server component (keeps `metadata`); the client Article holds the live
  // editors, playground, and animations. Suspense covers BamlCode's
  // highlighter promise.
  return (
    <Suspense>
      <Article />
    </Suspense>
  );
}
