import Image from 'next/image';
import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';

export const metadata = createMetadata({
  description:
    'The small team rebuilding language and tooling for a world where AI writes most of the code.',
  ogTitle: 'Our team',
  path: '/who-are-we',
  team: true,
  title: 'Who are we',
});

// The team page, in the homepage's BAML lane. The nav's Platform menu links it, as Company.

type TeamMember = {
  bio: string;
  image?: string;
  linkedin?: string;
  name: string;
  role: string;
  twitter?: string;
};

// Bios follow one shape on purpose: a single line of background, i.e. where
// each person came from, not what they "own" here. Uniform titles plus
// pedigree-only copy reads as one team of high-caliber people, not a set of
// silos.
const teamMembers: TeamMember[] = [
  {
    bio: 'Vaibhav previously built the on-device depth and face-unlock systems for Google’s Pixel 4, and worked on Microsoft’s HoloLens.',
    image: '/profile-vbv.jpeg',
    linkedin: 'https://www.linkedin.com/in/vaigup',
    name: 'Vaibhav Gupta',
    role: 'Co-founder & CEO',
    twitter: 'https://x.com/vaicode',
  },
  {
    bio: 'Aaron previously spent seven years at Amazon, scaling EC2’s internal monitoring and building live-streaming features for Prime Video and Twitch.',
    image: '/aaronv.jpg',
    linkedin: 'https://www.linkedin.com/in/aaron-villalpando-99284576/',
    name: 'Aaron Villalpando',
    role: 'Co-founder & CTO',
    twitter: 'https://x.com/aaronvi',
  },
  {
    bio: 'Sam previously worked on user identity and Cloud Firestore at Google, then built developer tooling at Trunk, after studying CS at Vanderbilt.',
    image: '/profile-sam.png',
    linkedin: 'https://www.linkedin.com/in/sxlijin/',
    name: 'Sam Lijin',
    role: 'Engineering',
    twitter: 'https://x.com/sxlijin',
  },
  {
    bio: 'Antonio has written an ACID-compliant database, an Nginx-style reverse proxy, and a memory allocator from scratch in Rust, and teaches it all to 180K+ subscribers on YouTube.',
    image: '/antonio.jpeg',
    linkedin: 'https://www.linkedin.com/in/antoniosarosi/',
    name: 'Antonio Sarosi',
    role: 'Engineering',
    twitter: 'https://x.com/antoniosarosi',
  },
  {
    bio: 'Paulo is a former Y Combinator founder who grew his startup past $2M in ARR, and was an early BAML user before he joined.',
    image: '/testimonials/people/paulo.png',
    name: 'Paulo Rossi',
    role: 'Engineering',
  },
  {
    bio: 'Kai built ere, a Rust crate that compiles regular expressions at build time into efficient, type-checked code, and a VS Code extension with over a million downloads, after an MS in computer science.',
    image: '/kai.png',
    name: 'Kai Orita',
    role: 'Engineering',
  },
  {
    bio: 'Avery revived the defunct LEGO Universe MMO with a custom C++ server emulator in high school, then wrote a full wgpu rendering backend from scratch and contributed to the Slint compiler while finishing a degree at UPenn.',
    image: '/avery.png',
    linkedin: 'https://www.linkedin.com/in/codeshaunted/',
    name: 'Avery Townsend',
    role: 'Engineering',
    twitter: 'https://x.com/codeshaunted',
  },
  {
    bio: 'Dhilan is a computer science intern from UT Austin who built agent-tries-baml, the system BAML uses to measure and improve how well AI agents write it.',
    image: '/dhilan.png',
    name: 'Dhilan Shah',
    role: 'Engineering',
    twitter: 'https://x.com/_dhilan_shah_',
  },
];

/** The team page: an intro with the team photo, a card for each member, and the open roles. */
export default function WhoAreWePage() {
  return (
    <PageShell>
      <section
        {...section('team_header')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-14 md:pb-20 grid lg:grid-cols-2 gap-10 lg:gap-16 items-center`}
      >
        <div>
          <h1 className="v4-h1">Our team</h1>
          <p className="mt-6 md:mt-8 v4-body text-ink-2">
            Coding has changed more in the last two years than in the twenty
            before it. The languages, the tools, and the type systems we all
            rely on were designed for a world where humans did the writing.
            We're a small team rebuilding those foundations for a world where AI
            does most of it.
          </p>
        </div>
        <Image
          alt="The Boundary team"
          className="w-full h-auto rounded-2xl border border-rule"
          height={1333}
          priority
          sizes="(min-width: 1024px) 600px, 100vw"
          src="/team.jpg"
          width={2000}
        />
      </section>

      <section
        {...section('team_people')}
        aria-labelledby="people"
        className={`${PAGE_WRAP} pb-16 md:pb-24`}
      >
        <h2
          className="pb-5 border-b border-rule v4-caption text-muted"
          id="people"
        >
          People building BAML
        </h2>
        <ul className="mt-8 grid md:grid-cols-2 gap-3 md:gap-4">
          {teamMembers.map((m) => (
            <li
              className="flex gap-5 rounded-2xl border border-rule bg-raised p-5 md:p-6"
              key={m.name}
            >
              <div className="shrink-0 flex flex-col items-center gap-3">
                {m.image ? (
                  <Image
                    alt=""
                    className="size-24 md:size-28 rounded-xl border border-rule object-cover"
                    height={224}
                    src={m.image}
                    width={224}
                  />
                ) : (
                  <span
                    aria-hidden
                    className="size-24 md:size-28 grid place-items-center rounded-xl border border-rule font-display text-[28px] text-clover"
                  >
                    {m.name
                      .split(' ')
                      .map((part) => part[0])
                      .join('')}
                  </span>
                )}
                {(m.twitter || m.linkedin) && (
                  <span className="flex gap-1">
                    {m.twitter && (
                      <a
                        aria-label={`${m.name} on X`}
                        className="tap grid place-items-center size-8 text-muted hover:text-clover transition-colors"
                        href={m.twitter}
                        rel="noreferrer"
                        target="_blank"
                      >
                        <XMark />
                      </a>
                    )}
                    {m.linkedin && (
                      <a
                        aria-label={`${m.name} on LinkedIn`}
                        className="tap grid place-items-center size-8 text-muted hover:text-clover transition-colors"
                        href={m.linkedin}
                        rel="noreferrer"
                        target="_blank"
                      >
                        <LinkedInMark />
                      </a>
                    )}
                  </span>
                )}
              </div>
              <div className="min-w-0">
                <p className="v4-caption text-muted">{m.role}</p>
                <h3 className="mt-2 font-display font-medium text-[22px] md:text-[24px] leading-[1.2] tracking-[-0.02em] text-head">
                  {m.name}
                </h3>
                <p className="mt-3 text-[16px] leading-[1.5] text-body">
                  {m.bio}
                </p>
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section
        {...section('team_join')}
        aria-labelledby="join"
        className="border-t border-rule"
      >
        <div className={`${PAGE_WRAP} py-20 md:py-28`}>
          <h2 className="v4-h2 max-w-[14em]" id="join">
            Work on the hard parts.
          </h2>
          <p className="mt-6 max-w-[36rem] v4-body text-ink-2">
            We're looking for people who can move between product and systems,
            write clearly, debug patiently, and care about making AI software
            less fragile.
          </p>
          <div className="mt-8 flex flex-wrap gap-2.5">
            <Button href="/jobs">Open roles</Button>
            <Button href={LINKS.github} variant="secondary">
              Star on GitHub
            </Button>
            <Button href="/discord" variant="secondary">
              Join Discord
            </Button>
          </div>
        </div>
      </section>
    </PageShell>
  );
}

/** The X logo, sized for a team member's social links. */
function XMark() {
  return (
    <svg
      aria-hidden
      fill="currentColor"
      height="14"
      viewBox="0 0 24 24"
      width="14"
    >
      <title>X</title>
      <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
    </svg>
  );
}

/** The LinkedIn logo, sized for a team member's social links. */
function LinkedInMark() {
  return (
    <svg
      aria-hidden
      fill="currentColor"
      height="15"
      viewBox="0 0 24 24"
      width="15"
    >
      <title>LinkedIn</title>
      <path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.268 2.37 4.268 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.063 2.063 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z" />
    </svg>
  );
}
