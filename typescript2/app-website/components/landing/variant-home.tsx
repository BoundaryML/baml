'use client';

import { AnimatePresence, motion } from 'motion/react';
import dynamic from 'next/dynamic';
import { DEFAULT_BAML, EXAMPLE_ARGS } from '@/playground/homepage-example';
import Image from 'next/image';
import Link from 'next/link';
import posthog from 'posthog-js';
import type React from 'react';
import { useEffect, useState } from 'react';

import { useIsDesktop } from '@/hooks/use-media-query';
import Marquee from '../magicui/marquee';
import { ScriptCopyBtn } from '../magicui/script-copy-btn';
import { Navbar } from '../navbar';

// Same editor/playground as the /learn decks: Monaco (baml-paper theme,
// BAML grammar, LSP diagnostics) + ExecutionPanel sharing one worker.
const LivePlayground = dynamic(
  () => import('@/app/learn2/_components/LivePlayground'),
  {
    loading: () => (
      <div className="flex h-full w-full items-center justify-center text-sm text-[#5C5852]">
        Loading playground...
      </div>
    ),
    ssr: false,
  },
);

const TRUST_LOGOS: { alt: string; src: string }[] = [
  { alt: 'SAP', src: '/testimonials/logos/sapLogo.png' },
  { alt: 'AWS', src: '/testimonials/logos/aws.png' },
  { alt: 'AMD', src: '/testimonials/logos/amd.png' },
  { alt: 'Cisco', src: '/testimonials/logos/cisco.png' },
  { alt: 'EY', src: '/EY.svg' },
  {
    alt: 'Product Hunt',
    src: '/testimonials/logos/product-hunt.png',
  },
  { alt: 'Aer Compliance', src: '/testimonials/logos/aer.png' },
  { alt: 'PMMI', src: '/testimonials/logos/pmmi.png' },
  {
    alt: 'Cerebral Valley',
    src: '/testimonials/logos/cerebral.png',
  },
];

const TrustMarquee = () => (
  <div style={{ marginTop: '56px', width: '100%' }}>
    <p
      style={{
        color: '#8A8580',
        fontSize: '11px',
        fontWeight: 500,
        letterSpacing: '0.08em',
        marginBottom: '12px',
        textTransform: 'uppercase',
      }}
    >
      Trusted by developers at
    </p>
    <div
      className="relative w-full overflow-hidden"
      style={{
        maskImage:
          'linear-gradient(to right, transparent 0%, #000 10%, #000 90%, transparent 100%)',
        WebkitMaskImage:
          'linear-gradient(to right, transparent 0%, #000 10%, #000 90%, transparent 100%)',
      }}
    >
      <Marquee className="[--duration:40s] [--gap:1.75rem] py-2">
        {TRUST_LOGOS.map((logo) => (
          <div
            className="flex h-12 w-24 flex-shrink-0 items-center justify-center"
            key={logo.alt}
          >
            <Image
              alt={logo.alt}
              className="max-h-8 max-w-full object-contain grayscale opacity-60 hover:grayscale-0 hover:opacity-100 transition-all duration-300"
              height={32}
              src={logo.src}
              width={128}
            />
          </div>
        ))}
      </Marquee>
    </div>
  </div>
);

const ROTATING_WORDS = [
  'verifying',
  'orchestrating',
  'testing',
  'debugging',
  'shipping',
  'using',
];
const HOLD_MS = 5500;
const TRANSITION_MS = 850;

const RotatingWord = () => {
  const [index, setIndex] = useState(0);
  const [reduced, setReduced] = useState(false);

  useEffect(() => {
    setReduced(window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  }, []);

  useEffect(() => {
    if (reduced) {
      return;
    }

    const timer = setInterval(() => {
      setIndex((current) => (current + 1) % ROTATING_WORDS.length);
    }, HOLD_MS);

    return () => {
      clearInterval(timer);
    };
  }, [reduced]);

  const transitionS = TRANSITION_MS / 1000;

  return (
    <span
      aria-atomic="true"
      aria-live="polite"
      style={{
        color: '#6D28D9',
        display: 'inline-block',
        fontSize: '0.94em',
        fontStyle: 'normal',
        fontWeight: 500,
        lineHeight: 1.12,
        minWidth: '10ch',
        overflow: 'hidden',
        position: 'relative',
        verticalAlign: 'baseline',
      }}
    >
      <AnimatePresence initial={false} mode="wait">
        <motion.span
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: '-0.9em' }}
          initial={{ opacity: 0, y: '0.9em' }}
          key={index}
          style={{ display: 'inline-block', whiteSpace: 'nowrap' }}
          transition={
            reduced
              ? { duration: 0 }
              : { duration: transitionS, ease: [0.22, 0.61, 0.36, 1] }
          }
        >
          {ROTATING_WORDS[index]}
        </motion.span>
      </AnimatePresence>
    </span>
  );
};

const customStyles = {
  botanicalAccent: {
    color: '#6D28D9',
    opacity: 0.6,
  },
  cm: { color: '#78716C' },
  codeContentArea: {
    backgroundColor: '#FAFAF9',
    display: 'flex',
  },
  codeScroll: {
    overflowX: 'auto' as const,
    padding: '16px',
    width: '100%',
  },
  codeWindow: {
    backgroundColor: '#FAFAF9',
    border: '1px solid #D9D3C4',
    borderRadius: '6px',
    boxShadow: '0 20px 40px rgba(0,0,0,0.05)',
    display: 'flex',
    flexDirection: 'column' as const,
    overflow: 'hidden',
    width: '100%',
  },
  codeWindowAccent: {
    backgroundColor: '#FAFAF9',
    border: '1px solid #6D28D9',
    borderRadius: '6px',
    boxShadow: 'none',
    display: 'flex',
    flexDirection: 'column' as const,
    overflow: 'hidden',
    width: '100%',
  },
  codeWindowNoShadow: {
    backgroundColor: '#FAFAF9',
    border: '1px solid #D9D3C4',
    borderRadius: '6px',
    boxShadow: 'none',
    display: 'flex',
    flexDirection: 'column' as const,
    overflow: 'hidden',
    width: '100%',
  },
  container: {
    backgroundColor: '#FBF7ED',
    backgroundImage:
      'radial-gradient(circle at 1px 1px, rgba(42,37,32,0.035) 1px, transparent 0)',
    backgroundSize: '18px 18px',
    color: '#1A1612',
    display: 'flex',
    flexDirection: 'column',
    fontSize: '14px',
    lineHeight: '1.5',
    margin: '0 auto',
    maxWidth: '1600px',
    width: '100%',
  } as React.CSSProperties,
  ctaContainer: {
    alignItems: 'center',
    display: 'flex',
    gap: '12px',
    marginTop: '2rem',
  },
  ctaLink: {
    alignItems: 'center',
    color: 'inherit',
    cursor: 'pointer',
    display: 'inline-flex',
    fontSize: '18px',
    fontWeight: 500,
    textDecoration: 'none',
    transition: 'color 0.2s ease',
  } as React.CSSProperties,
  ctaLinkSvg: {
    marginLeft: '12px',
    transition: 'transform 0.2s ease',
  },
  dotGreen: {
    backgroundColor: '#27C93F',
    border: '1px solid rgba(0,0,0,0.1)',
    borderRadius: '50%',
    display: 'inline-block',
    height: '10px',
    width: '10px',
  },
  dotRed: {
    backgroundColor: '#FF5F56',
    border: '1px solid rgba(0,0,0,0.1)',
    borderRadius: '50%',
    display: 'inline-block',
    height: '10px',
    width: '10px',
  },
  dotYellow: {
    backgroundColor: '#FFBD2E',
    border: '1px solid rgba(0,0,0,0.1)',
    borderRadius: '50%',
    display: 'inline-block',
    height: '10px',
    width: '10px',
  },
  exhibitGrid: {
    borderTop: '1px solid #D9D3C4',
    display: 'grid',
    gridTemplateColumns: '1fr 1fr',
  },
  exhibitHeader: {
    alignItems: 'baseline',
    display: 'grid',
    gridTemplateColumns: '1fr auto',
    padding: '40px 48px 20px',
  },
  exhibitPanel: {
    backgroundColor: 'rgba(255,255,255,0.1)',
    padding: '48px',
  },
  exhibitPanelLeft: {
    backgroundColor: 'rgba(255,255,255,0.1)',
    borderRight: '1px solid #D9D3C4',
    padding: '48px',
  },
  exhibitTitle: {
    fontSize: '2rem',
    fontWeight: 500,
  } as React.CSSProperties,
  featureIndex: {
    borderTop: '2px solid #1A1612',
    display: 'flex',
    flexDirection: 'column' as const,
  },
  fn: { color: '#0D9488' },
  h1: {
    color: '#1A1612',
    fontSize: 'clamp(2rem, 4.5vw, 3.5rem)',
    fontWeight: 600,
    letterSpacing: '-0.03em',
    lineHeight: '1.02',
    marginBottom: '1.5rem',
  } as React.CSSProperties,
  h2: {
    fontSize: 'clamp(2rem, 4vw, 3.5rem)',
    fontWeight: 500,
    letterSpacing: '-0.02em',
    lineHeight: '1.2',
  } as React.CSSProperties,
  hero: {
    borderBottom: '1px solid #D9D3C4',
    display: 'grid',
    gridTemplateColumns: '496px 1fr',
    minHeight: '720px',
  } as React.CSSProperties,
  heroLeft: {
    backgroundColor: '#ffffff',
    display: 'flex',
    flexDirection: 'column' as const,
    justifyContent: 'space-between',
    padding: '48px 48px 0',
  },
  heroMeta: {
    alignItems: 'center',
    display: 'flex',
    fontSize: '12px',
    justifyContent: 'space-between',
    marginBottom: '4rem',
    textTransform: 'uppercase' as const,
  },
  heroMetaBottom: {
    alignItems: 'center',
    display: 'flex',
    fontSize: '12px',
    justifyContent: 'space-between',
    marginBottom: 0,
    marginTop: '4rem',
    textTransform: 'uppercase' as const,
  },
  heroRight: {
    alignItems: 'stretch',
    backgroundColor: 'rgba(255,255,255,0.3)',
    display: 'flex',
    justifyContent: 'center',
    overflow: 'hidden',
    padding: 0,
  } as React.CSSProperties,
  indexMeta: {
    color: '#6D28D9',
    fontSize: '12px',
    textAlign: 'right' as const,
  },
  indexNum: {
    color: '#78716C',
    fontSize: '12px',
  },
  indexRow: {
    alignItems: 'baseline',
    borderBottom: '1px solid #D9D3C4',
    cursor: 'default',
    display: 'grid',
    gridTemplateColumns: '60px 1fr 100px',
    padding: '16px 0',
    transition: 'background-color 0.2s ease',
  },
  indexTitle: {
    fontSize: '18px',
    fontWeight: 500,
  },
  kw: { color: '#8B5CF6', fontWeight: 500 },
  lineNumbers: {
    backgroundColor: '#F5F5F4',
    borderRight: '1px solid #D9D3C4',
    color: '#A8A29E',
    fontSize: '13px',
    lineHeight: '1.5',
    padding: '16px 12px',
    textAlign: 'right' as const,
    userSelect: 'none' as const,
  },
  lineNumbersAccent: {
    backgroundColor: '#F5F5F4',
    borderRight: '1px solid #6D28D9',
    color: '#6D28D9',
    fontSize: '13px',
    lineHeight: '1.5',
    padding: '16px 12px',
    textAlign: 'right' as const,
    userSelect: 'none' as const,
  },
  logo: {
    fontWeight: 600,
    padding: '0 16px',
    paddingLeft: 0,
  } as React.CSSProperties,
  nav: {
    alignItems: 'center',
    borderBottom: '1px solid #D9D3C4',
    columnGap: '16px',
    display: 'grid',
    fontSize: '15px',
    gridTemplateColumns: 'auto 1fr auto auto auto',
    letterSpacing: '0.05em',
    padding: '16px 24px',
    textTransform: 'uppercase',
  } as React.CSSProperties,
  navDiv: {
    padding: '0 16px',
  } as React.CSSProperties,
  navItem: {
    padding: '0 16px',
    textAlign: 'right' as const,
  },
  p: {
    color: '#5C5852',
    fontSize: '17px',
    fontWeight: 400,
    lineHeight: '1.6',
    marginBottom: '1rem',
    maxWidth: '480px',
  },
  panelHeader: {
    color: '#6D28D9',
    display: 'flex',
    fontSize: '12px',
    justifyContent: 'space-between',
    marginBottom: '24px',
    textTransform: 'uppercase' as const,
  },
  pre: {
    color: '#1A1612',
    fontFamily:
      "'IBM Plex Mono', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace",
    fontSize: '13px',
    margin: 0,
  },
  refMark: {
    color: '#6D28D9',
    fontWeight: 'normal' as const,
  },
  root: {
    '--accent': '#6D28D9',
    '--accent-hover': '#5B21B6',
    '--bg': '#FBF7ED',
    '--border': '#D9D3C4',
    '--fg': '#1A1612',
    '--secondary': '#6D28D9',
    '--syn-comment': '#78716C',
    '--syn-green': '#059669',
    '--syn-purple': '#8B5CF6',
    '--syn-string': '#B45309',
    '--syn-teal': '#0D9488',
  } as React.CSSProperties,
  st: { color: '#B45309' },
  statementP: {
    fontSize: 'clamp(1.1rem, 2vw, 1.6rem)',
    fontWeight: 300,
    lineHeight: '1.4',
    marginBottom: '0.5rem',
  } as React.CSSProperties,
  statementSection: {
    borderBottom: '1px solid #D9D3C4',
    padding: '48px 48px',
  },
  statementText: {
    margin: '0 auto 48px',
    maxWidth: '700px',
    textAlign: 'center' as const,
  },
  ty: { color: '#059669' },
  windowChrome: {
    alignItems: 'center',
    backgroundColor: '#F5F5F4',
    borderBottom: '1px solid #D9D3C4',
    display: 'grid',
    gridTemplateColumns: '80px 1fr 80px',
    padding: '12px 16px',
  },
  windowDots: {
    display: 'flex',
    gap: '6px',
  },
  windowTab: {
    color: '#57534E',
    fontSize: '12px',
    textAlign: 'center' as const,
  },
};

const WindowDots = () => (
  <div style={customStyles.windowDots}>
    <span style={customStyles.dotRed} />
    <span style={customStyles.dotYellow} />
    <span style={customStyles.dotGreen} />
  </div>
);

const HeroCodeWindow = () => (
  <div style={customStyles.codeWindow}>
    <div style={customStyles.windowChrome}>
      <WindowDots />
      <div style={customStyles.windowTab}>extract_receipt.baml</div>
      <div />
    </div>
    <div style={customStyles.codeContentArea}>
      <div style={customStyles.lineNumbers}>
        {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((n) => (
          <div key={n}>{n}</div>
        ))}
      </div>
      <div style={customStyles.codeScroll}>
        <pre style={customStyles.pre}>
          <code>
            <span style={customStyles.kw}>class</span>{' '}
            <span style={customStyles.ty}>Receipt</span> {'{'}
            {'\n'} total <span style={customStyles.ty}>float</span>{' '}
            @description(
            <span style={customStyles.st}>"Final amount paid"</span>){'\n'}{' '}
            items <span style={customStyles.ty}>Item[]</span>
            {'\n'} date <span style={customStyles.ty}>string</span>{' '}
            @description(
            <span style={customStyles.st}>"YYYY-MM-DD"</span>){'\n'}
            {'}'}
            {'\n'}
            {'\n'}
            <span style={customStyles.kw}>function</span>{' '}
            <span style={customStyles.fn}>ExtractReceipt</span>(img:{' '}
            <span style={customStyles.ty}>Image</span>) -&gt;{' '}
            <span style={customStyles.ty}>Receipt</span> {'{'}
            {'\n'} <span style={customStyles.kw}>client</span> GPT4o
            {'\n'} <span style={customStyles.kw}>prompt</span> #"
            {'\n'} Extract the items and total from this receipt.
            {'\n'} {'{{ ctx.output_format }}'}
            {'\n'}
            {'\n'} Receipt: {'{{ img }}'}
            {'\n'} "#
            {'\n'}
            {'}'}
          </code>
        </pre>
      </div>
    </div>
  </div>
);

type InstallPath = 'claude' | 'codex';

const claudeInstallCommands = [
  '/plugin marketplace add BoundaryML/baml-skill',
  '/plugin install baml@boundaryml-baml',
];
const codexInstallCommands = ['codex plugin add boundaryml/baml'];
const installOptions: {
  id: InstallPath;
  label: string;
  icon?: string;
  commands: string[];
}[] = [
  {
    commands: claudeInstallCommands,
    icon: '/Claude Color SVG.svg',
    id: 'claude',
    label: 'Claude plugin',
  },
  {
    commands: codexInstallCommands,
    icon: '/Codex Color.svg',
    id: 'codex',
    label: 'Codex plugin',
  },
];

const HeroSection = () => {
  const [installPath, setInstallPath] = useState<InstallPath>('claude');
  const isDesktop = useIsDesktop();

  const selected =
    installOptions.find((option) => option.id === installPath) ??
    installOptions[0];

  return (
    <section className="hero-responsive" style={customStyles.hero}>
      <div style={customStyles.heroLeft}>
        <div>
          <h1 style={customStyles.h1}>
            A language for
            <br />
            <RotatingWord />
            <br />
            AI
          </h1>
          <p style={customStyles.p}>
            Python and Java were built for humans. BAML was built for agents.
          </p>
          {/* Not a link — the arrow just points at the live playground that
              sits in this hero. */}
          <p
            style={{
              alignItems: 'center',
              color: '#6D28D9',
              display: 'inline-flex',
              fontSize: '17px',
              fontWeight: 500,
              gap: '6px',
              margin: '0 0 1.75rem',
            }}
          >
            Try it in your browser
            <span aria-hidden>→</span>
          </p>
          <div style={customStyles.ctaContainer}>
            <div className="w-full max-w-xl">
              <p
                style={{
                  color: '#8A8580',
                  fontSize: '18px',
                  fontWeight: 500,
                  letterSpacing: '0.08em',
                  marginBottom: '20px',
                  textTransform: 'uppercase',
                }}
              >
                Install
              </p>
              <div
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 8,
                  maxWidth: 400,
                  width: '100%',
                }}
              >
                {selected.commands.map((command) => (
                  <ScriptCopyBtn
                    className="block w-full max-w-none"
                    codeLanguage="bash"
                    commandMap={{ bash: command } as const}
                    darkTheme="none"
                    key={command}
                    lightTheme="none"
                    onCopy={(copied) =>
                      posthog.capture('install_command_copied', {
                        install_path: installPath,
                        command: copied,
                      })
                    }
                    showMultiplePackageOptions={false}
                  />
                ))}
              </div>
              <div className="mt-3 flex gap-2">
                {installOptions.map((opt) => (
                  <button
                    className="rounded-md px-3 py-2 text-sm font-medium transition-colors flex items-center gap-1.5 cursor-pointer"
                    key={opt.id}
                    onClick={() => {
                      setInstallPath(opt.id);
                      posthog.capture('install_path_selected', {
                        install_path: opt.id,
                      });
                    }}
                    style={
                      installPath === opt.id
                        ? {
                            background: '#1A1612',
                            border: '1px solid #1A1612',
                            color: '#fff',
                          }
                        : {
                            background: 'transparent',
                            border: '1px solid #D9D3C4',
                            color: '#5C5852',
                          }
                    }
                    type="button"
                  >
                    {opt.icon && (
                      <Image
                        alt={opt.label}
                        className="size-4"
                        height={16}
                        src={opt.icon}
                        width={16}
                      />
                    )}
                    {opt.label}
                  </button>
                ))}
              </div>
              <TrustMarquee />
            </div>
          </div>
        </div>
      </div>
      <div
        className="hero-right-responsive relative"
        style={customStyles.heroRight}
      >
        <div className="absolute inset-0 min-h-0 overflow-hidden">
          {/* CSS decides what's VISIBLE (so desktop never flashes the mobile
              screenshot before hydration), `isDesktop` only decides whether
              to MOUNT the heavy playground (wasm worker) at all. */}
          <div className="hidden h-full w-full lg:block">
            {isDesktop ? (
              <LivePlayground
                argsByFunction={EXAMPLE_ARGS}
                fill
                initialCode={DEFAULT_BAML}
                initialFunction="Main"
                initialSidebarOpen={false}
                initialTab="run"
              />
            ) : (
              <div className="flex h-full w-full items-center justify-center text-sm text-[#5C5852]">
                Loading playground...
              </div>
            )}
          </div>
          <Link
            className="group relative block h-full w-full lg:hidden"
            href="/how-the-playground-works"
          >
            <Image
              alt="BAML playground preview"
              className="object-cover object-top"
              fill
              sizes="100vw"
              src="/bamlPlaygroundLightScreenshot.png"
            />
            <div className="absolute inset-x-0 bottom-0 bg-white/90 px-4 py-3 text-center text-[13px] text-[#1A1612]">
              Open on desktop to try the playground live
            </div>
          </Link>
        </div>
      </div>
    </section>
  );
};

const FeatureIndex = () => {
  const features = [
    {
      meta: 'Authorship',
      num: '01',
      title: 'TypeScript shape, agents write it fluently',
      version: 'core',
    },
    {
      meta: 'Compiler',
      num: '02',
      title: 'Error-correcting parser (bex_sap)',
      version: 'core',
    },
    {
      meta: 'Language',
      num: '03',
      title: 'Tagged union tool dispatch via match',
      version: 'core',
    },
    {
      meta: 'Runtime',
      num: '04',
      title: 'Custom VM with epoch based async',
      version: 'BexVM',
    },
  ];

  const [hoveredRow, setHoveredRow] = useState<number | null>(null);

  return (
    <div style={customStyles.featureIndex}>
      {features.map((f, i) => (
        <div
          className="index-row-responsive"
          key={f.num}
          onMouseEnter={() => setHoveredRow(i)}
          onMouseLeave={() => setHoveredRow(null)}
          style={{
            ...customStyles.indexRow,
            backgroundColor:
              hoveredRow === i ? 'rgba(255,255,255,0.4)' : 'transparent',
          }}
        >
          <span style={customStyles.indexNum}>{f.num}</span>
          <span style={customStyles.indexTitle}>{f.title}</span>
          <span style={customStyles.indexMeta}>
            {f.meta} <sup>{f.version}</sup>
          </span>
        </div>
      ))}
    </div>
  );
};

const StatementSection = () => (
  <section className="statement-section" style={customStyles.statementSection}>
    <div style={customStyles.statementText}>
      <p style={customStyles.statementP}>Agents write it. Agents run in it.</p>
      <p style={customStyles.statementP}>
        <strong>The agent is the program</strong>.
      </p>
    </div>
    <FeatureIndex />
  </section>
);

const LegacyCodeWindow = () => (
  <div style={customStyles.codeWindowNoShadow}>
    <div style={customStyles.codeContentArea}>
      <div style={customStyles.lineNumbers}>
        {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((n) => (
          <div key={n}>{n}</div>
        ))}
      </div>
      <div style={customStyles.codeScroll}>
        <pre style={customStyles.pre}>
          <code>
            <span style={customStyles.kw}>type</span>{' '}
            <span style={customStyles.ty}>Tool</span> ={'\n'}
            {'  | { kind: '}
            <span style={customStyles.st}>'answer'</span>
            {'; text: '}
            <span style={customStyles.ty}>string</span>
            {' }'}
            {'\n'}
            {'  | { kind: '}
            <span style={customStyles.st}>'readFile'</span>
            {'; path: '}
            <span style={customStyles.ty}>string</span>
            {' }'}
            {'\n'}
            {'  | { kind: '}
            <span style={customStyles.st}>'runBash'</span>
            {'; cmd: '}
            <span style={customStyles.ty}>string</span>
            {' };'}
            {'\n'}
            {'\n'}
            <span style={customStyles.kw}>async function</span>{' '}
            <span style={customStyles.fn}>dispatch</span>(t:{' '}
            <span style={customStyles.ty}>Tool</span>) {'{'}
            {'\n'}{' '}
            <span style={customStyles.cm}>
              {'// no exhaustiveness check at the language level'}
            </span>
            {'\n'} <span style={customStyles.kw}>if</span> (t.kind ==={' '}
            <span style={customStyles.st}>'answer'</span>){' '}
            <span style={customStyles.kw}>return</span> t.text;
            {'\n'} <span style={customStyles.kw}>if</span> (t.kind ==={' '}
            <span style={customStyles.st}>'readFile'</span>){' '}
            <span style={customStyles.kw}>return</span>{' '}
            <span style={customStyles.fn}>read</span>(t.path);
            {'\n'}{' '}
            <span style={customStyles.cm}>
              {'// Zod schema lives elsewhere — kept in sync by hand'}
            </span>
            {'\n'}
            {'}'}
          </code>
        </pre>
      </div>
    </div>
  </div>
);

const BamlCodeWindow = () => (
  <div style={customStyles.codeWindowAccent}>
    <div style={customStyles.codeContentArea}>
      <div style={customStyles.lineNumbersAccent}>
        {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12].map((n) => (
          <div key={n}>{n}</div>
        ))}
      </div>
      <div style={customStyles.codeScroll}>
        <pre style={customStyles.pre}>
          <code>
            <span style={customStyles.kw}>class</span>{' '}
            <span style={customStyles.ty}>Answer</span>
            {'   { text '}
            <span style={customStyles.ty}>string</span>
            {' }'}
            {'\n'}
            <span style={customStyles.kw}>class</span>{' '}
            <span style={customStyles.ty}>ReadFile</span>
            {' { path '}
            <span style={customStyles.ty}>string</span>
            {' }'}
            {'\n'}
            <span style={customStyles.kw}>class</span>{' '}
            <span style={customStyles.ty}>RunBash</span>
            {'  { command '}
            <span style={customStyles.ty}>string</span>
            {' }'}
            {'\n'}
            <span style={customStyles.kw}>type</span>{' '}
            <span style={customStyles.ty}>Tool</span> ={' '}
            <span style={customStyles.ty}>Answer</span> |{' '}
            <span style={customStyles.ty}>ReadFile</span> |{' '}
            <span style={customStyles.ty}>RunBash</span>
            {'\n'}
            {'\n'}
            <span style={customStyles.kw}>function</span>{' '}
            <span style={customStyles.fn}>dispatch</span>(tool:{' '}
            <span style={customStyles.ty}>Tool</span>) -&gt;{' '}
            <span style={customStyles.ty}>string</span> {'{'}
            {'\n'} <span style={customStyles.kw}>match</span> (tool) {'{'}
            {'\n'}
            {'    a: '}
            <span style={customStyles.ty}>Answer</span>
            {'    => a.text,'}
            {'\n'}
            {'    r: '}
            <span style={customStyles.ty}>ReadFile</span>
            {'  => baml.fs.'}
            <span style={customStyles.fn}>read</span>(r.path),
            {'\n'}
            {'    b: '}
            <span style={customStyles.ty}>RunBash</span>
            {'   => baml.sys.'}
            <span style={customStyles.fn}>shell</span>(b.command),
            {'\n'}
            {'  }'}
            {'\n'}
            {'}'}
          </code>
        </pre>
      </div>
    </div>
  </div>
);

const ExhibitSection = () => (
  <section>
    <div className="exhibit-header" style={customStyles.exhibitHeader}>
      <h2 style={customStyles.exhibitTitle}>The agent is the program.</h2>
    </div>
    <div className="exhibit-grid-responsive" style={customStyles.exhibitGrid}>
      <div style={customStyles.exhibitPanelLeft}>
        <div style={customStyles.panelHeader}>
          <span>TypeScript</span>
          <span>dispatch.ts</span>
        </div>
        <LegacyCodeWindow />
      </div>
      <div style={customStyles.exhibitPanel}>
        <div style={customStyles.panelHeader}>
          <span>BAML</span>
          <span>dispatch.baml</span>
        </div>
        <BamlCodeWindow />
      </div>
    </div>
  </section>
);

export function VariantHome() {
  useEffect(() => {
    const style = document.createElement('style');
    style.textContent = `
      @media (max-width: 1024px) {
        .hero-responsive { grid-template-columns: minmax(0, 1fr) !important; }
        .hero-responsive > div { min-width: 0 !important; }
        .hero-right-responsive { border-top: 1px solid #D9D3C4; border-right: none !important; }
        .exhibit-grid-responsive { grid-template-columns: minmax(0, 1fr) !important; }
        .exhibit-grid-responsive > div { min-width: 0 !important; border-right: none !important; }
      }
      @media (max-width: 600px) {
        .index-row-responsive { grid-template-columns: 40px 1fr !important; }
        .hero-right-responsive { display: none !important; }
        .hero-responsive > div:first-child { padding: 32px 20px 0 !important; }
        .statement-section { padding: 36px 20px !important; }
        .exhibit-header { padding: 28px 20px 16px !important; }
        .exhibit-grid-responsive > div { padding: 28px 20px !important; }
      }
    `;
    document.head.appendChild(style);
    return () => {
      document.head.removeChild(style);
    };
  }, []);

  return (
    <div style={customStyles.root as React.CSSProperties}>
      <div style={customStyles.container}>
        <Navbar />
        <HeroSection />
      </div>
    </div>
  );
}
