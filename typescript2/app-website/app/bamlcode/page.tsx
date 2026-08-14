import Link from 'next/link';
import { createMetadata } from '@/app/_lib/metadata';
import { ProblemsBoard } from './_components/ProblemsBoard';
import { PROBLEMS } from './_lib/problems-index';
import './bamlcode.css';

export const metadata = createMetadata({
  description:
    'Practice algorithm problems written in BAML, graded instantly in your browser. No API key required.',
  ogTitle: 'LeetCode for BAML',
  path: '/bamlcode',
  title: 'bamlcode: LeetCode for BAML',
});

export default function BamlCodeIndex() {
  const items = PROBLEMS.map((p) => ({
    category: p.category,
    difficulty: p.difficulty,
    id: p.id,
    slug: p.slug,
    title: p.title,
  }));

  return (
    <div className="bc-app">
      <aside className="bc-sidebar">
        <Link className="bc-sidebar-logo font-mono" href="/bamlcode">
          bamlcode
        </Link>
        <nav className="bc-sidebar-nav font-mono">
          <span className="bc-sidebar-item bc-sidebar-active">Problems</span>
          <span className="bc-sidebar-item bc-sidebar-muted">Study Plan</span>
          <span className="bc-sidebar-item bc-sidebar-muted">Explore</span>
        </nav>
        <div className="bc-sidebar-section font-mono">My Lists</div>
        <nav className="bc-sidebar-nav font-mono">
          <span className="bc-sidebar-item bc-sidebar-active">Blind 75</span>
        </nav>
        <p className="bc-sidebar-foot">
          Every solution is a real{' '}
          <span className="font-mono">baml_language</span> program, graded live
          in your browser.
        </p>
      </aside>

      <ProblemsBoard problems={items} />
    </div>
  );
}
