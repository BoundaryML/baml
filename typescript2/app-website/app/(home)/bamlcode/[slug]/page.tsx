import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { section } from '@/components/home/analytics';
import Nav from '@/components/home/nav';
import { SectionViews } from '@/components/home/track';
import { DifficultyTag } from '../_components/difficulty';
import { FeedbackWidget } from '../_components/FeedbackWidget';
import { Statement } from '../_components/Statement';
import { SolveLayout } from '../_components/solve-layout';
import WorkbenchLazy from '../_components/WorkbenchLazy';
import { getProblem, PROBLEMS } from '../_lib/problems-index';
import '@/app/(home)/styles/v3.css';
import '@/app/(home)/styles/home.css';
import '../bamlcode.css';

/** The slug of every problem, so each problem page is built ahead of time. */
export function generateStaticParams() {
  return PROBLEMS.map((p) => ({ slug: p.slug }));
}

/** The problem page's title and description, or a bare title for an unknown slug. */
export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const problem = getProblem(slug);
  if (!problem) return { title: 'bamlcode' };
  return {
    description: `Solve "${problem.title}" in BAML, graded live in your browser.`,
    title: `${problem.title} · bamlcode`,
  };
}

/** A problem page: the statement beside the workbench, under the homepage's nav. */
export default async function ProblemPage({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  const problem = getProblem(slug);
  if (!problem) notFound();

  // A full-screen workbench, so it skips PageShell: the footer would sit under a
  // viewport-high app. It keeps the homepage's Nav and PageShell's page classes.
  return (
    <div className="v3-page v4-page bc-solve-page">
      <Nav />
      <main className="bc-solve" id="main">
        <header
          {...section('bamlcode_problem_header')}
          className="bc-solve-head"
        >
          <Link className="bc-back" href="/bamlcode">
            ← Problems
          </Link>
          <div className="bc-solve-title">
            <span className="bc-num font-mono">#{problem.id}</span>
            <h1>{problem.title}</h1>
            <DifficultyTag difficulty={problem.difficulty} />
            <span className="bc-cat v4-caption">{problem.category}</span>
          </div>
        </header>

        <div {...section('bamlcode_workbench')} className="bc-solve-main">
          <SolveLayout
            statement={
              <>
                <Statement markdown={problem.statement} />
                <div className="bc-signature">
                  <div className="bc-sig-label v4-caption">Implement</div>
                  <pre className="font-mono">{problem.signature}</pre>
                </div>
                <FeedbackWidget slug={slug} />
              </>
            }
            workbench={<WorkbenchLazy problem={problem} />}
          />
        </div>
      </main>
      <SectionViews />
    </div>
  );
}
