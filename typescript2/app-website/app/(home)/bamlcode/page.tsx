import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
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

// The problem list, in the homepage's BAML lane. It isn't linked from the nav or the footer.
export default function BamlCodeIndex() {
  const items = PROBLEMS.map((p) => ({
    category: p.category,
    difficulty: p.difficulty,
    id: p.id,
    slug: p.slug,
    title: p.title,
  }));

  return (
    <PageShell>
      <section
        {...section('bamlcode_header')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-10 md:pb-14`}
      >
        <p className="v4-caption text-accent">LeetCode for BAML</p>
        <h1 className="mt-4 v4-h1">bamlcode</h1>
        <p className="mt-6 max-w-[40ch] v4-body text-ink-2">
          Every solution is a real{' '}
          <span className="font-mono text-[0.9em]">baml_language</span> program,
          graded live in your browser.
        </p>
      </section>

      <section
        {...section('bamlcode_problems')}
        aria-label="Problems"
        className={`${PAGE_WRAP} pb-16 md:pb-24`}
      >
        <div className="bc-app">
          <aside className="bc-sidebar">
            <nav aria-label="bamlcode" className="bc-sidebar-nav">
              <span className="bc-sidebar-item bc-sidebar-active">
                Problems
              </span>
              <span className="bc-sidebar-item bc-sidebar-muted">
                Study plan
              </span>
              <span className="bc-sidebar-item bc-sidebar-muted">Explore</span>
            </nav>
            <div className="bc-sidebar-section v4-caption">My lists</div>
            <nav aria-label="My lists" className="bc-sidebar-nav">
              <span className="bc-sidebar-item bc-sidebar-active">
                Blind 75
              </span>
            </nav>
          </aside>

          <ProblemsBoard problems={items} />
        </div>
      </section>
    </PageShell>
  );
}
