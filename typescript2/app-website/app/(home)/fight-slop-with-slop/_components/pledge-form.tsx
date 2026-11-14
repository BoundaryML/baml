'use client';

import { useMutation } from 'convex/react';
import { type FormEvent, useEffect, useRef, useState } from 'react';
import { api } from '@/convex/_generated/api';

const MAX_DESC = 280;

const SHARE_URL = 'https://waronslop.com';

// The site's button styles (components/home/ui.tsx `Button`), for <button>s:
// that component only renders links. The accent fill is for the one main
// action in view.
const BTN =
  'inline-flex items-center gap-2 whitespace-nowrap px-5 h-[46px] rounded-lg text-[15px] font-medium active:translate-y-px transition disabled:opacity-60';
const BTN_PRIMARY = `${BTN} bg-accent text-on-accent hover:bg-accent-hover`;
const BTN_SECONDARY = `${BTN} border border-accent/35 text-accent hover:border-accent/70`;

// ── Shareable card ───────────────────────────────────────────────────────────
// Paint the user's dispatch onto a 1200×675 (Twitter card ratio) canvas they can
// download and attach to a post. X's intent URL can't carry an image, so we hand
// them the PNG + a prefilled composer.
/**
 * Splits text into lines that fit `maxWidth` in the context's current font,
 * keeping its paragraph breaks.
 */
function wrapLines(
  ctx: CanvasRenderingContext2D,
  text: string,
  maxWidth: number,
) {
  const lines: string[] = [];
  for (const paragraph of text.split('\n')) {
    let line = '';
    for (const word of paragraph.split(/\s+/)) {
      const test = line ? `${line} ${word}` : word;
      if (line && ctx.measureText(test).width > maxWidth) {
        lines.push(line);
        line = word;
      } else {
        line = test;
      }
    }
    lines.push(line);
  }
  return lines;
}

/**
 * The card follows the page: the theme's ground, rule, and ink, Lexend for
 * the title, Geist for the dispatch, and Geist Pixel Square for the footer
 * line. Canvas can't read CSS classes, so the tokens are read off <html> at
 * draw time.
 */
function theme() {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim();
  return {
    accent: v('--accent'),
    body: v('--body'),
    display: v('--font-lexend'),
    ground: v('--ground'),
    head: v('--head'),
    ink2: v('--ink-2'),
    pixel: v('--font-geist-pixel'),
    rule: v('--rule'),
    sans: v('--font-geist'),
  };
}

/** Paints the share card for a name and dispatch onto a 1200 by 675 canvas. */
function drawCard(canvas: HTMLCanvasElement, name: string, dispatch: string) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const t = theme();
  const W = 1200;
  const H = 675;
  canvas.width = W;
  canvas.height = H;

  ctx.fillStyle = t.ground;
  ctx.fillRect(0, 0, W, H);
  ctx.strokeStyle = t.rule;
  ctx.lineWidth = 4;
  ctx.strokeRect(28, 28, W - 56, H - 56);

  ctx.textBaseline = 'top';

  // Title, in the display face.
  ctx.fillStyle = t.head;
  ctx.font = `600 56px ${t.display}, sans-serif`;
  ctx.fillText('fight slop with slop', 80, 84);

  // Dispatch, wrapped.
  ctx.fillStyle = t.ink2;
  ctx.font = `400 40px ${t.sans}, sans-serif`;
  const lines = wrapLines(ctx, `“${dispatch}”`, W - 160);
  let y = 220;
  for (const line of lines.slice(0, 7)) {
    ctx.fillText(line, 80, y);
    y += 56;
  }

  // Signature.
  ctx.fillStyle = t.body;
  ctx.font = `500 32px ${t.display}, sans-serif`;
  ctx.fillText(name, 80, y + 18);

  // Footer.
  ctx.font = `400 26px ${t.pixel}, monospace`;
  ctx.fillStyle = t.accent;
  ctx.fillText('waronslop.com', 80, H - 96);
  ctx.fillStyle = t.body;
  const handle = '@boundaryml';
  ctx.fillText(handle, W - 80 - ctx.measureText(handle).width, H - 96);
}

/** A preview of the share card, with buttons to post to X and download it. */
function ShareCard({ name, dispatch }: { name: string; dispatch: string }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let cancelled = false;
    const render = () => {
      if (!cancelled) drawCard(canvas, name || 'Anonymous', dispatch);
    };
    render();
    // Repaint once the fonts are ready so the card isn't drawn in a fallback.
    if (typeof document !== 'undefined' && 'fonts' in document) {
      const t = theme();
      Promise.all([
        document.fonts.load(`600 56px ${t.display}`),
        document.fonts.load(`400 40px ${t.sans}`),
        document.fonts.load(`400 26px ${t.pixel}`),
      ])
        .then(render)
        .catch(() => {});
    }
    return () => {
      cancelled = true;
    };
  }, [name, dispatch]);

  function download() {
    canvasRef.current?.toBlob((blob) => {
      if (!blob) return;
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'war-on-slop.png';
      a.click();
      URL.revokeObjectURL(url);
    }, 'image/png');
  }

  function shareToX() {
    const text = `${dispatch}\n\nFighting slop with slop ⚔️`;
    const intent = `https://twitter.com/intent/tweet?text=${encodeURIComponent(text)}&url=${encodeURIComponent(SHARE_URL)}`;
    window.open(intent, '_blank', 'noopener,noreferrer');
  }

  return (
    <div>
      <p className="font-display text-[22px] font-medium text-head">
        Enlisted.
      </p>
      <p className="mt-1 text-body">
        Your pledge is on the wall below. Share your card:
      </p>

      <canvas
        className="mt-4 w-full rounded-2xl border border-rule"
        ref={canvasRef}
        style={{ aspectRatio: '1200 / 675' }}
      />

      <div className="mt-4 flex flex-wrap items-center gap-3">
        <button className={BTN_PRIMARY} onClick={shareToX} type="button">
          Share on
          <svg
            aria-hidden="true"
            className="h-4 w-4 fill-current"
            viewBox="0 0 24 24"
          >
            <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231 5.45-6.231Zm-1.161 17.52h1.833L7.084 4.126H5.117L17.083 19.77Z" />
          </svg>
        </button>
        <button className={BTN_SECONDARY} onClick={download} type="button">
          Download card
        </button>
      </div>
      <p className="mt-3 text-sm text-muted">
        Download the card, then attach it to your post.
      </p>
    </div>
  );
}

/** The pledge form. Once a pledge is sent, it shows the share card instead. */
export default function PledgeForm() {
  const submit = useMutation(api.slopPledges.submit);
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [description, setDescription] = useState('');
  const [website, setWebsite] = useState(''); // honeypot
  const [status, setStatus] = useState<'idle' | 'sending' | 'done' | 'error'>(
    'idle',
  );
  const [error, setError] = useState('');
  const [submitted, setSubmitted] = useState<{
    name: string;
    description: string;
  } | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (status === 'sending') return;
    setStatus('sending');
    setError('');
    try {
      await submit({ description, email, name, website });
      setSubmitted({ description, name });
      setStatus('done');
      setName('');
      setEmail('');
      setDescription('');
    } catch (err) {
      setStatus('error');
      setError(err instanceof Error ? err.message : 'Something went wrong.');
    }
  }

  const inputClass =
    'w-full border-0 border-b border-rule bg-transparent px-0 py-3 text-[17px] text-ink outline-none transition placeholder:text-muted focus:border-accent';

  if (status === 'done') {
    return (
      <div className="bg-transparent py-2 text-left">
        <ShareCard
          dispatch={submitted?.description ?? ''}
          name={submitted?.name ?? ''}
        />
        <button
          className="mt-5 text-sm font-medium text-accent underline underline-offset-4 hover:text-accent-hover"
          onClick={() => setStatus('idle')}
          type="button"
        >
          Add another
        </button>
      </div>
    );
  }

  return (
    <form className="w-full" onSubmit={onSubmit}>
      <div className="grid gap-5 sm:grid-cols-2">
        <label htmlFor="pledge-name">
          <span className="sr-only">Name</span>
          <input
            autoComplete="name"
            className={inputClass}
            id="pledge-name"
            maxLength={80}
            name="name"
            onChange={(e) => setName(e.target.value)}
            placeholder="Name"
            required
            value={name}
          />
        </label>
        <label htmlFor="pledge-email">
          <span className="sr-only">Email</span>
          <input
            autoComplete="email"
            className={inputClass}
            id="pledge-email"
            maxLength={200}
            name="email"
            onChange={(e) => setEmail(e.target.value)}
            placeholder="Email"
            required
            type="email"
            value={email}
          />
        </label>
      </div>
      <div className="mt-5">
        <label htmlFor="pledge-description">
          <span className="sr-only">How are you fighting slop with slop?</span>
          <textarea
            className={`${inputClass} resize-none leading-relaxed`}
            id="pledge-description"
            maxLength={MAX_DESC}
            name="description"
            onChange={(e) => setDescription(e.target.value.slice(0, MAX_DESC))}
            placeholder="How are you fighting slop with slop?"
            required
            rows={2}
            value={description}
          />
        </label>
        <div className="mt-1 text-right font-mono text-xs text-muted">
          {description.length}/{MAX_DESC}
        </div>
      </div>

      {/* honeypot — hidden from real users */}
      <input
        aria-hidden="true"
        autoComplete="off"
        className="hidden"
        name="website"
        onChange={(e) => setWebsite(e.target.value)}
        tabIndex={-1}
        type="text"
        value={website}
      />

      {status === 'error' && (
        <p className="mt-3 text-sm text-destructive">{error}</p>
      )}

      <div className="mt-4 flex justify-center sm:justify-end">
        <button
          className={`group ${BTN_PRIMARY}`}
          disabled={status === 'sending'}
          type="submit"
        >
          {status === 'sending' ? 'Sharing' : 'Share'}
          <svg
            aria-hidden="true"
            className={`h-4 w-4 fill-none stroke-current stroke-2 ${status === 'sending' ? 'animate-paper-plane-send' : 'transition group-hover:translate-x-0.5 group-hover:-translate-y-0.5'}`}
            viewBox="0 0 24 24"
          >
            <path d="M3.5 11.5 20 4l-7.5 16-2-7-7-1.5Z" />
            <path d="m10.5 13 4-4" />
          </svg>
        </button>
      </div>
    </form>
  );
}
