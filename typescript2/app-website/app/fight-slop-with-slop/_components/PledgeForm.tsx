'use client';

import { FormEvent, useEffect, useRef, useState } from 'react';
import { useMutation } from 'convex/react';
import { api } from '@/convex/_generated/api';

const MAX_DESC = 280;

const SHARE_URL = 'https://waronslop.com';

// ── Shareable card ───────────────────────────────────────────────────────────
// Paint the user's dispatch onto a 1200×675 (Twitter card ratio) canvas they can
// download and attach to a post. X's intent URL can't carry an image, so we hand
// them the PNG + a prefilled composer.
function wrapLines(ctx: CanvasRenderingContext2D, text: string, maxWidth: number) {
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

function drawCard(canvas: HTMLCanvasElement, name: string, dispatch: string) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const W = 1200;
  const H = 675;
  canvas.width = W;
  canvas.height = H;

  ctx.fillStyle = '#fffef5';
  ctx.fillRect(0, 0, W, H);
  ctx.strokeStyle = 'rgba(31,30,27,0.25)';
  ctx.lineWidth = 4;
  ctx.strokeRect(28, 28, W - 56, H - 56);

  ctx.textBaseline = 'top';

  // Title — the Bitcount display face.
  ctx.fillStyle = '#1f1e1b';
  ctx.font = '700 52px "Bitcount Grid Double", ui-monospace, monospace';
  ctx.fillText('fight slop with slop', 80, 84);

  // Dispatch — readable serif, wrapped.
  ctx.font = '400 42px Georgia, "Times New Roman", serif';
  const lines = wrapLines(ctx, `“${dispatch}”`, W - 160);
  let y = 220;
  for (const line of lines.slice(0, 7)) {
    ctx.fillText(line, 80, y);
    y += 58;
  }

  // Signature.
  ctx.fillStyle = '#5e5a4f';
  ctx.font = 'italic 700 34px Georgia, serif';
  ctx.fillText(`— ${name}`, 80, y + 18);

  // Footer.
  ctx.font = '400 26px "Bitcount Grid Double", ui-monospace, monospace';
  ctx.fillStyle = '#7c3aed';
  ctx.fillText('waronslop.com', 80, H - 96);
  ctx.fillStyle = '#5e5a4f';
  const handle = '@boundaryml';
  ctx.fillText(handle, W - 80 - ctx.measureText(handle).width, H - 96);
}

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
    // Repaint once the display font is ready so the title isn't a fallback.
    if (typeof document !== 'undefined' && 'fonts' in document) {
      document.fonts
        .load('700 52px "Bitcount Grid Double"')
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
    <div className="tweet-font">
      <p className="text-lg font-bold text-wos-ink">Enlisted.</p>
      <p className="mt-1 text-wos-ink-2">Your pledge is on the wall below. Share your card:</p>

      <canvas
        ref={canvasRef}
        className="mt-4 w-full rounded-xl border border-wos-ink/15 shadow-sm"
        style={{ aspectRatio: '1200 / 675' }}
      />

      <div className="mt-4 flex flex-wrap items-center gap-3">
        <button
          onClick={shareToX}
          className="inline-flex items-center gap-2 rounded-full bg-wos-ink px-4 py-2 text-sm font-bold text-wos-cream-hi transition hover:opacity-90"
        >
          Share on
          <svg aria-hidden="true" viewBox="0 0 24 24" className="h-4 w-4 fill-current">
            <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231 5.45-6.231Zm-1.161 17.52h1.833L7.084 4.126H5.117L17.083 19.77Z" />
          </svg>
        </button>
        <button
          onClick={download}
          className="text-sm font-bold text-wos-accent underline underline-offset-4 hover:text-wos-accent-deep"
        >
          Download card
        </button>
      </div>
      <p className="mt-2 text-xs text-wos-ink-2/70">Download the card, then attach it to your post.</p>
    </div>
  );
}

export default function PledgeForm() {
  const submit = useMutation(api.slopPledges.submit);
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [description, setDescription] = useState('');
  const [website, setWebsite] = useState(''); // honeypot
  const [status, setStatus] = useState<'idle' | 'sending' | 'done' | 'error'>('idle');
  const [error, setError] = useState('');
  const [submitted, setSubmitted] = useState<{ name: string; description: string } | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (status === 'sending') return;
    setStatus('sending');
    setError('');
    try {
      await submit({ name, email, description, website });
      setSubmitted({ name, description });
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
    'w-full border-0 border-b border-wos-ink/25 bg-transparent px-0 py-3 text-[17px] text-wos-ink outline-none transition placeholder:text-wos-ink-2/70 focus:border-wos-accent';

  if (status === 'done') {
    return (
      <div className="bg-transparent py-2 text-left">
        <ShareCard name={submitted?.name ?? ''} dispatch={submitted?.description ?? ''} />
        <button
          onClick={() => setStatus('idle')}
          className="tweet-font mt-4 text-sm font-bold text-wos-accent underline underline-offset-4 hover:text-wos-accent-deep"
        >
          Add another
        </button>
      </div>
    );
  }

  return (
    <form
      onSubmit={onSubmit}
      className="tweet-font w-full bg-transparent"
    >
      <div className="grid gap-5 sm:grid-cols-2">
        <label htmlFor="pledge-name">
          <span className="sr-only">Name</span>
          <input
            id="pledge-name"
            name="name"
            autoComplete="name"
            className={inputClass}
            placeholder="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={80}
            required
          />
        </label>
        <label htmlFor="pledge-email">
          <span className="sr-only">Email</span>
          <input
            id="pledge-email"
            name="email"
            autoComplete="email"
            className={inputClass}
            type="email"
            placeholder="Email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            maxLength={200}
            required
          />
        </label>
      </div>
      <div className="mt-5">
        <label htmlFor="pledge-description">
          <span className="sr-only">How are you fighting slop with slop?</span>
          <textarea
            id="pledge-description"
            name="description"
            rows={2}
            className={`${inputClass} resize-none leading-relaxed`}
            placeholder="How are you fighting slop with slop?"
            value={description}
            onChange={(e) => setDescription(e.target.value.slice(0, MAX_DESC))}
            maxLength={MAX_DESC}
            required
          />
        </label>
        <div className="mt-1 text-right text-xs font-bold text-wos-ink/45">
          {description.length}/{MAX_DESC}
        </div>
      </div>

      {/* honeypot — hidden from real users */}
      <input
        type="text"
        name="website"
        tabIndex={-1}
        autoComplete="off"
        value={website}
        onChange={(e) => setWebsite(e.target.value)}
        className="hidden"
        aria-hidden="true"
      />

      {status === 'error' && <p className="mt-3 text-sm text-red-700">{error}</p>}

      <div className="-mt-4 flex justify-center sm:mt-2 sm:justify-end">
        <button
          type="submit"
          disabled={status === 'sending'}
          className="group inline-flex items-center gap-2 bg-transparent py-1 text-[15px] font-bold text-wos-accent transition hover:text-wos-accent-deep disabled:opacity-60"
        >
          {status === 'sending' ? 'Sharing' : 'Share'}
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            className={`hidden h-4 w-4 fill-none stroke-current stroke-2 sm:block ${status === 'sending' ? 'animate-paper-plane-send' : 'transition group-hover:translate-x-0.5 group-hover:-translate-y-0.5'}`}
          >
            <path d="M3.5 11.5 20 4l-7.5 16-2-7-7-1.5Z" />
            <path d="m10.5 13 4-4" />
          </svg>
        </button>
      </div>
    </form>
  );
}
