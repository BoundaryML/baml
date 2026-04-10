import { type Metadata } from 'next';
import { redirect } from 'next/navigation';
export const metadata: Metadata = {
  title: 'BAML - The language for cognitive coding',
  description:
    'Cognitive coding for AI—code that agents write, software that humans trust.',
};

export default function Home() {
  redirect('/');
}
