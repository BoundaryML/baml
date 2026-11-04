import { createMetadata } from '@/app/_lib/metadata';
import { DiscordCard, EapCard } from '@/components/community-cards';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Cmd } from '@/components/home/ui';
// CONTENT PARITY: keep substantive copy and commands in sync with
// content/quickstart.md. Update both representations in the same change.
import { TryBaml } from './_components/try-baml';

export const metadata = createMetadata({
  description:
    'Install BAML and its toolchain, agent skills, and editor extensions.',
  ogSubtitle: 'Mac, Windows, and Linux.',
  ogTitle: 'Install BAML in seconds',
  path: '/quickstart',
  title: 'Quickstart',
});

// The install page, in the homepage's BAML lane. It isn't linked from the nav or the footer.

const HANDY_LINKS = [
  {
    href: '/explore',
    title: 'Explore BAML',
    value: 'see the language and code',
  },
  {
    external: true,
    href: 'https://github.com/boundaryml/baml-demos',
    title: 'Demo repo',
    value: 'github.com/boundaryml/baml-demos',
  },
  {
    href: '/explore#describe',
    title: 'Docs',
    value: 'how baml describe works',
  },
];

const H2 =
  'font-display font-medium text-[24px] leading-[1.2] tracking-[-0.02em] text-head';

export default function QuickstartPage() {
  return (
    <PageShell>
      <section
        {...section('quickstart_install')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-12 md:pb-16`}
      >
        <div className="max-w-[45rem]">
          <h1 className="v4-h1">Quickstart</h1>
          <p className="mt-6 md:mt-8 v4-body text-ink-2">
            BAML is the programming language for the AI era. BAML feels like
            typescript, but without many of the sins of javascript. It&rsquo;s
            incrementally adoptable, typesafe, and has agent first tooling.
          </p>
          {/* The install unit spans the column so it lines up with the cards below. */}
          <div className="mt-10 md:mt-12 [&>div]:max-w-none">
            <TryBaml />
          </div>
        </div>
      </section>

      <section
        {...section('quickstart_next')}
        className={`${PAGE_WRAP} pb-16 md:pb-24`}
      >
        <div className="max-w-[45rem] flex flex-col gap-12">
          <div className="grid sm:grid-cols-2 gap-3">
            <EapCard />
            <DiscordCard />
          </div>

          <div>
            <h2 className={H2}>Handy links</h2>
            <ul className="mt-4 grid sm:grid-cols-2 gap-3">
              {HANDY_LINKS.map((link) => (
                <li key={link.title}>
                  <a
                    className="group flex h-full flex-col gap-1 rounded-xl border border-rule bg-raised px-4 py-3.5 transition-colors hover:border-accent/50"
                    href={link.href}
                    {...(link.external
                      ? { rel: 'noopener noreferrer', target: '_blank' }
                      : {})}
                  >
                    <span className="flex items-center justify-between gap-3 text-[15px] font-medium text-head">
                      {link.title}
                      <span
                        aria-hidden
                        className="text-muted transition-colors group-hover:text-accent"
                      >
                        ↗
                      </span>
                    </span>
                    <span className="text-[14px] leading-normal text-body break-words">
                      {link.value}
                    </span>
                  </a>
                </li>
              ))}
            </ul>
          </div>

          <div>
            <h2 className={H2}>Keeping BAML updated</h2>
            <p className="mt-4 border-l-2 border-rule pl-4 text-[16px] leading-[1.6] text-body">
              The <Cmd>baml</Cmd> wrapper is only a version manager, so it
              rarely needs updating. Update it through your package manager (
              <Cmd>brew upgrade baml</Cmd>) or with <Cmd>baml self-update</Cmd>.
              Everything here is discoverable from the binary itself, so agents
              shouldn&rsquo;t have any trouble. If they do, let us know.
            </p>
          </div>
        </div>
      </section>
    </PageShell>
  );
}
