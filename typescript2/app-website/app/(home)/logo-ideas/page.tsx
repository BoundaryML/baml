import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { BoundaryLambLogo } from '@/components/home/boundary-logo';
import { PageShell } from '@/components/home/page-shell';
import { Generated } from './generated';
import {
  Clover,
  DoubleB,
  Field,
  Horn,
  LambHead,
  Leap,
  LitAlt,
  Lockup,
  type MarkProps,
  PostAndHorn,
  PromptB,
  RingB,
  Slant,
  Split,
} from './marks';

// A review board of logo ideas for Boundary, BAML and the two together. Not linked from the site.
// Like the site's other pages, it sits in PageShell for the shared nav and footer.

export const metadata: Metadata = {
  robots: { follow: false, index: false },
  title: 'Logo ideas',
};

type Idea = {
  name: string;
  why: string;
  Mark: (props: MarkProps) => ReactNode;
};

type Lane = {
  title: string;
  about: string;
  word: 'boundary' | 'baml';
  /** The glow behind each app icon. */
  glow: string;
  ideas: Idea[];
};

const LANES: Lane[] = [
  {
    about: 'The company and the platform, in Pasture green.',
    glow: 'rgb(130 181 113 / 0.28)',
    ideas: [
      {
        Mark: Slant,
        name: 'Slant',
        why: 'A disc cut into three slanted bands that fade downward: the layers of one stack, with a boundary between each. The slant comes from the alt mark.',
      },
      {
        Mark: RingB,
        name: 'Ring b',
        why: 'A lowercase b from two shapes: a round-ended post and a ring. The post is the fence post from the gate.',
      },
      {
        Mark: Field,
        name: 'Field',
        why: 'A disc whose lower half thins into rows, like the pasture in the site’s paintings running to the horizon.',
      },
      {
        Mark: LitAlt,
        name: 'Lit alt mark',
        why: 'The alt mark as it is, with a gradient on each bar so it reads as one folded ribbon. This is a finish, not a new shape.',
      },
    ],
    title: 'Boundary',
    word: 'boundary',
  },
  {
    about: 'The language, in Clover purple.',
    glow: 'rgb(180 132 234 / 0.3)',
    ideas: [
      {
        Mark: Horn,
        name: 'Horn',
        why: 'A lowercase b drawn in one line whose bowl winds inward like a ram’s horn. It keeps the lamb without drawing an animal.',
      },
      {
        Mark: LambHead,
        name: 'Lamb head',
        why: 'The lamb rebuilt from circles and arcs instead of pixels: three circles of wool, two ears and a rounded face.',
      },
      {
        Mark: Clover,
        name: 'Clover',
        why: 'Four translucent discs that overlap into a four-leaf clover, after the name of BAML’s color.',
      },
      {
        Mark: PromptB,
        name: 'Prompt b',
        why: 'A solid lowercase b with a prompt chevron cut out as its counter.',
      },
    ],
    title: 'BAML',
    word: 'baml',
  },
  {
    about:
      'Both brands in one mark. Green parts are Boundary and purple parts are BAML.',
    glow: 'rgb(202 170 242 / 0.22)',
    ideas: [
      {
        Mark: Leap,
        name: 'Leap',
        why: 'The lamb’s jump over the gate, reduced to an arc, a dot and a post.',
      },
      {
        Mark: DoubleB,
        name: 'Double b',
        why: 'Both names start with b, so two lowercase b’s link through each other, one in each color.',
      },
      {
        Mark: Split,
        name: 'Split',
        why: 'One disc and one slanted cut: Boundary above the line and BAML below it.',
      },
      {
        Mark: PostAndHorn,
        name: 'Post and horn',
        why: 'The Horn b in two colors: the post is Boundary and the horn is BAML.',
      },
    ],
    title: 'Boundary and BAML',
    word: 'boundary',
  },
];

function Heading({ children }: { children: ReactNode }) {
  return (
    <h2
      className="font-medium text-4xl tracking-[-0.02em]"
      style={{ fontFamily: 'var(--font-lexend)' }}
    >
      {children}
    </h2>
  );
}

// The mark as an app icon: a dark rounded tile, lit from the top, with the lane's glow behind it.
function AppIcon({ glow, idea }: { glow: string; idea: Idea }) {
  const { Mark } = idea;
  return (
    <div className="relative shrink-0">
      <div
        aria-hidden
        className="absolute -inset-6 rounded-full blur-2xl"
        style={{ background: glow }}
      />
      <div
        className="relative flex size-36 items-center justify-center rounded-[32px]"
        style={{
          background: 'linear-gradient(160deg, #26262A 0%, #0C0C0E 70%)',
          boxShadow:
            'inset 0 1px 0 rgb(255 255 255 / 0.18), inset 0 0 0 1px rgb(255 255 255 / 0.07), 0 18px 40px rgb(0 0 0 / 0.6)',
        }}
      >
        <Mark className="size-[84px]" />
      </div>
    </div>
  );
}

function IdeaCard({ idea, lane }: { idea: Idea; lane: Lane }) {
  const { Mark } = idea;
  return (
    <article className="flex flex-col border-white/12 border-t pt-8">
      <div className="flex flex-wrap items-center gap-x-10 gap-y-8 px-2 py-6">
        <AppIcon glow={lane.glow} idea={idea} />
        <div className="flex flex-col gap-6">
          <Lockup
            className="h-11 w-auto text-white"
            Mark={Mark}
            word={lane.word}
          />
          <div className="flex items-center gap-4 text-white">
            <Mark className="size-8" mono />
            <Mark className="size-4" mono />
            <span className="flex size-8 items-center justify-center rounded-md bg-white text-black">
              <Mark className="size-5" mono />
            </span>
            <span
              className="text-sm text-white/40"
              style={{ fontFamily: 'var(--font-geist-pixel)' }}
            >
              one color, 32 and 16 px
            </span>
          </div>
        </div>
      </div>
      <h3
        className="mt-4 font-medium text-2xl text-white"
        style={{ fontFamily: 'var(--font-lexend)' }}
      >
        {idea.name}
      </h3>
      <p className="mt-2 max-w-[52ch] text-base text-white/65 leading-relaxed">
        {idea.why}
      </p>
    </article>
  );
}

export default function LogoIdeas() {
  return (
    <PageShell>
      <div className="bg-[#0F0F0F] px-6 pt-16 pb-28 text-white sm:px-10">
        <div className="mx-auto max-w-[1180px]">
          <h1
            className="font-semibold text-5xl tracking-[-0.03em] sm:text-6xl"
            style={{ fontFamily: 'var(--font-lexend)' }}
          >
            Logo ideas
          </h1>
          <p className="mt-5 max-w-[62ch] text-lg text-white/65 leading-relaxed">
            Logos drawn by code, then twelve picked and tuned by hand, then the
            lamb wordmark.
          </p>

          <section className="mt-20">
            <Heading>Generated</Heading>
            <p className="mt-3 mb-8 max-w-[62ch] text-base text-white/65 leading-relaxed">
              Fifteen families of marks. Each family is a function that draws a
              mark from random numbers, and each row shows eight results.
              Shuffle to draw a new set of 120.
            </p>
            <Generated />
          </section>

          <section className="mt-28">
            <Heading>Picked by hand</Heading>
            <p className="mt-3 max-w-[62ch] text-base text-white/65 leading-relaxed">
              One mark from most families, shown as an app icon, beside the
              existing wordmark, and in one color at small sizes.
            </p>
          </section>

          {LANES.map((lane) => (
            <section className="mt-14" key={lane.title}>
              <h3
                className="font-medium text-3xl"
                style={{ fontFamily: 'var(--font-lexend)' }}
              >
                {lane.title}
              </h3>
              <p className="mt-3 mb-10 max-w-[62ch] text-base text-white/65 leading-relaxed">
                {lane.about}
              </p>
              <div className="grid gap-x-14 gap-y-12 lg:grid-cols-2">
                {lane.ideas.map((idea) => (
                  <IdeaCard idea={idea} key={idea.name} lane={lane} />
                ))}
              </div>
            </section>
          ))}

          <section className="mt-24">
            <Heading>The lamb wordmark</Heading>
            <p className="mt-3 mb-10 max-w-[62ch] text-base text-white/65 leading-relaxed">
              The alt mark’s main piece with the pixel lamb rising out of it.
            </p>
            <div className="flex h-56 items-center justify-center border-white/12 border-y px-8">
              <BoundaryLambLogo className="h-28 w-auto max-w-full text-white" />
            </div>
          </section>
        </div>
      </div>
    </PageShell>
  );
}
