import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';

// Landing spot for every "Read more" link on the homepage until the real
// deep-dive articles ship. One page, one promise: the writing is human.

export const metadata = createMetadata({
  description:
    'BAML deep-dive articles are coming, written by humans, strictly no AI.',
  ogTitle: 'Tech docs',
  path: '/techdocs',
  title: 'Tech docs',
});

/** The tech docs placeholder: a heading and a note that the articles are coming. */
export default function TechDocsPage() {
  return (
    <PageShell>
      <section
        {...section('techdocs_header')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-20 md:pb-32`}
      >
        <h1 className="v4-h1">Tech docs coming soon</h1>
        <p className="mt-6 md:mt-8 max-w-[40rem] v4-body text-ink-2">
          We know there are a lot of toy &ldquo;programming languages&rdquo;
          coming out these days. We&rsquo;ve been building BAML for a while. We
          know our stuff. These articles are strictly no-AI. We explain our type
          system, our observability features, and more.
        </p>
      </section>
    </PageShell>
  );
}
