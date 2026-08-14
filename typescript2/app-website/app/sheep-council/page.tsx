import { createMetadata } from '@/app/_lib/metadata';
import { Navbar } from '@/components/navbar';
import { CouncilGate } from './council-gate';

export const metadata = createMetadata({
  description: 'Members only. Speak the words to enter the chamber.',
  indexable: false,
  ogTitle: 'The Sheep Council',
  path: '/sheep-council',
  title: 'The Sheep Council',
});

const CSS = `
.sc-root { position: relative; min-height: 100vh; display: flex; align-items: center;
  justify-content: center; padding: 24px; overflow: hidden; }
/* hand-drawn council chamber; sits behind everything */
.sc-bg { position: fixed; inset: 0; z-index: 0;
  background: url('/sheep-council.png') center center / cover no-repeat; }
/* site-palette scrim (dark #1A1612) so the cream card reads on top */
.sc-overlay { position: fixed; inset: 0; z-index: 1;
  background: radial-gradient(ellipse at center, rgba(26,22,18,0.40) 0%, rgba(26,22,18,0.78) 100%); }

/* transparent "glass" login — the chamber shows through it */
.sc-card { position: relative; z-index: 2; width: 100%; max-width: 440px;
  background: rgba(20,16,12,0.34); backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px);
  border: 1px solid rgba(255,255,255,0.16); border-radius: 16px;
  padding: 40px 36px; box-shadow: 0 24px 70px rgba(0,0,0,0.45); }
.sc-form { display: flex; flex-direction: column; gap: 14px; }
.sc-kicker { color: rgba(251,247,237,0.65); font-size: 12px; letter-spacing: 0.14em; text-transform: uppercase;
  font-weight: 500; margin: 0; }
.sc-title { color: #FBF7ED; font-size: 28px; font-weight: 600; letter-spacing: -0.02em;
  line-height: 1.12; margin: 0; text-shadow: 0 1px 12px rgba(0,0,0,0.45); }
.sc-sub { color: rgba(251,247,237,0.82); font-size: 15px; line-height: 1.5; margin: 0 0 6px; }
.sc-sheep { font-size: 56px; line-height: 1; }
.sc-label { display: flex; flex-direction: column; gap: 6px; color: rgba(251,247,237,0.82); font-size: 13px;
  font-weight: 500; }
.sc-input { background: rgba(255,255,255,0.10); border: 1px solid rgba(255,255,255,0.24); border-radius: 9px;
  padding: 11px 13px; font-size: 15px; color: #FBF7ED; font-family: inherit; outline: none; width: 100%; box-sizing: border-box; }
.sc-input::placeholder { color: rgba(251,247,237,0.45); }
.sc-input:focus { border-color: #a78bfa; box-shadow: 0 0 0 3px rgba(167,139,250,0.22); }
.sc-textarea { resize: vertical; }
.sc-error { color: #fca5a5; font-size: 13px; margin: 0; }
.sc-btn { margin-top: 6px; background: #FBF7ED; color: #1A1612; border: 0; border-radius: 10px;
  padding: 12px 18px; font-size: 15px; font-weight: 600; cursor: pointer; transition: background 0.12s ease; }
.sc-btn:hover { background: #fff; }
.sc-btn:disabled { opacity: 0.45; cursor: not-allowed; }
/* one-field wizard: nav row, step dots, slide animation */
.sc-nav { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 8px; }
.sc-btn-ghost { background: transparent; border: 1px solid rgba(255,255,255,0.28); color: rgba(251,247,237,0.82);
  border-radius: 10px; padding: 11px 14px; font-size: 14px; font-weight: 500; cursor: pointer;
  transition: border-color 0.12s ease, color 0.12s ease; }
.sc-btn-ghost:hover:not(:disabled) { border-color: #fff; color: #fff; }
.sc-btn-ghost:disabled { opacity: 0; cursor: default; pointer-events: none; }
.sc-dots { display: flex; gap: 7px; }
.sc-dots button { width: 9px; height: 9px; padding: 0; border: 1px solid rgba(255,255,255,0.45); border-radius: 50%;
  background: transparent; cursor: pointer; transition: background 0.12s ease, transform 0.12s ease; }
.sc-dots button.done { background: rgba(255,255,255,0.45); }
.sc-dots button.on { background: #FBF7ED; border-color: #FBF7ED; transform: scale(1.15); }
@keyframes scSlide { from { opacity: 0; transform: translateX(14px); } to { opacity: 1; transform: none; } }
.sc-slide { animation: scSlide 0.2s ease; }
/* small "why we ask" note under a field */
.sc-hint { margin: -6px 0 0; color: rgba(251,247,237,0.62); font-size: 12.5px; line-height: 1.4; }
/* address auto-finisher: input wrapper + suggestion dropdown */
.sc-ac { position: relative; }
.sc-ac-list { position: absolute; top: calc(100% + 6px); left: 0; right: 0; z-index: 10; list-style: none;
  margin: 0; padding: 6px; max-height: 236px; overflow-y: auto;
  background: rgba(20,16,12,0.92); backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px);
  border: 1px solid rgba(255,255,255,0.18); border-radius: 10px; box-shadow: 0 18px 50px rgba(0,0,0,0.5); }
.sc-ac-list li { margin: 0; }
.sc-ac-item { display: block; width: 100%; text-align: left; background: transparent; border: 0; border-radius: 7px;
  padding: 9px 11px; font-size: 14px; line-height: 1.35; color: rgba(251,247,237,0.9); font-family: inherit;
  cursor: pointer; transition: background 0.1s ease, color 0.1s ease; }
.sc-ac-item:hover { background: rgba(167,139,250,0.22); color: #FBF7ED; }
/* keep the site navbar above the fixed background + overlay */
.sc-navwrap { position: relative; z-index: 5; }
`;

export default function SheepCouncilPage() {
  return (
    <>
      {/* eslint-disable-next-line react/no-danger */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <div aria-hidden className="sc-bg" />
      <div aria-hidden className="sc-overlay" />
      <div className="sc-navwrap">
        <Navbar />
      </div>
      <div className="sc-root">
        <CouncilGate />
      </div>
    </>
  );
}
