import type { Metadata } from 'next';
import '../learn2/learn2.css';
import '../learn3/learn3.css';
import './learn4.css';
import { Story } from './_components/Story';

export const metadata: Metadata = {
  title: 'BAML — a programming language for AI software',
  description:
    'Model calls are typed functions. Errors are inferred. Concurrency needs no async. Like TypeScript — without the sins of JavaScript.',
};

export default function Learn4Page() {
  // Server component (keeps `metadata`); the client Story holds the live
  // editors, playground, and animations.
  return <Story />;
}
