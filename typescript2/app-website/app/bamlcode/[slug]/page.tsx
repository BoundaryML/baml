import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { FeedbackWidget } from '../_components/FeedbackWidget';
import { SolveLayout } from '../_components/SolveLayout';
import { Statement } from '../_components/Statement';
import WorkbenchLazy from '../_components/WorkbenchLazy';
import { getProblem, PROBLEMS } from '../_lib/problems-index';
import '../bamlcode.css';

const DIFFICULTY_CLASS: Record<string, string> = {
  Easy: 'bc-diff-easy',
  Hard: 'bc-diff-hard',
  Medium: 'bc-diff-medium',
};

export function generateStaticParams() {
  return PROBLEMS.map((p) => ({ slug: p.slug }));
}

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

export default async function ProblemPage({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  const problem = getProblem(slug);
  if (!problem) notFound();

  return (
    <div className="bc-solve">
      <header className="bc-solve-head">
        <Link className="bc-back font-mono" href="/bamlcode">
          ← problems
        </Link>
        <div className="bc-solve-title">
          <span className="bc-num font-mono">#{problem.id}</span>
          <h1>{problem.title}</h1>
          <span
            className={`bc-badge ${DIFFICULTY_CLASS[problem.difficulty] ?? ''}`}
          >
            {problem.difficulty}
          </span>
          <span className="bc-cat font-mono">{problem.category}</span>
        </div>
      </header>

      <SolveLayout
        statement={
          <>
            <Statement markdown={problem.statement} />
            <div className="bc-signature">
              <div className="bc-sig-label font-mono">Implement</div>
              <pre className="font-mono">{problem.signature}</pre>
            </div>
            <FeedbackWidget slug={slug} />
          </>
        }
        workbench={<WorkbenchLazy problem={problem} />}
      />
    </div>
  );
}
