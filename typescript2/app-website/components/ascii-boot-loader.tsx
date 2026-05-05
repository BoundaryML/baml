'use client';

import { useEffect, useState } from 'react';

const BAML_ASCII = String.raw`          _____                    _____                    _____                    _____  
         /\    \                  /\    \                  /\    \                  /\    \ 
        /::\    \                /::\    \                /::\____\                /::\____\
       /::::\    \              /::::\    \              /::::|   |               /:::/    /
      /::::::\    \            /::::::\    \            /:::::|   |              /:::/    / 
     /:::/\:::\    \          /:::/\:::\    \          /::::::|   |             /:::/    /  
    /:::/__\:::\    \        /:::/__\:::\    \        /:::/|::|   |            /:::/    /   
   /::::\   \:::\    \      /::::\   \:::\    \      /:::/ |::|   |           /:::/    /    
  /::::::\   \:::\    \    /::::::\   \:::\    \    /:::/  |::|___|______    /:::/    /     
 /:::/\:::\   \:::\ ___\  /:::/\:::\   \:::\    \  /:::/   |::::::::\    \  /:::/    /      
/:::/__\:::\   \:::|    |/:::/  \:::\   \:::\____\/:::/    |:::::::::\____\/:::/____/       
\:::\   \:::\  /:::|____|\::/    \:::\  /:::/    /\::/    / ~~~~~/:::/    /\:::\    \       
 \:::\   \:::\/:::/    /  \/____/ \:::\/:::/    /  \/____/      /:::/    /  \:::\    \      
  \:::\   \::::::/    /            \::::::/    /               /:::/    /    \:::\    \     
   \:::\   \::::/    /              \::::/    /               /:::/    /      \:::\    \    
    \:::\  /:::/    /               /:::/    /               /:::/    /        \:::\    \   
     \:::\/:::/    /               /:::/    /               /:::/    /          \:::\    \  
      \::::::/    /               /:::/    /               /:::/    /            \:::\    \ 
       \::::/    /               /:::/    /               /:::/    /              \:::\____\
        \::/____/                \::/    /                \::/    /                \::/    /
         ~~                       \/____/                  \/____/                  \/____/`;

const TYPE_MS = 1050;
const HOLD_MS = 260;
const FADE_MS = 260;
const HAS_SHOWN_KEY = 'baml-ascii-boot-loader-shown';

export function AsciiBootLoader() {
  const [visible, setVisible] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const [shownChars, setShownChars] = useState<number | null>(null);

  useEffect(() => {
    if (window.sessionStorage.getItem(HAS_SHOWN_KEY) === '1') {
      return;
    }

    window.sessionStorage.setItem(HAS_SHOWN_KEY, '1');
    setVisible(true);

    const reduced = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches;

    if (reduced) {
      setShownChars(BAML_ASCII.length);
      const hold = window.setTimeout(() => setLeaving(true), 120);
      const done = window.setTimeout(() => setVisible(false), 120 + FADE_MS);
      return () => {
        window.clearTimeout(hold);
        window.clearTimeout(done);
      };
    }

    let startedAt = 0;
    let frame = 0;
    let holdTimer = 0;
    let doneTimer = 0;

    const tick = (now: number) => {
      if (startedAt === 0) startedAt = now;
      const progress = Math.min((now - startedAt) / TYPE_MS, 1);
      const nextChars = Math.ceil(BAML_ASCII.length * progress);
      setShownChars(nextChars);

      if (progress < 1) {
        frame = window.requestAnimationFrame(tick);
        return;
      }

      holdTimer = window.setTimeout(() => {
        setLeaving(true);
        doneTimer = window.setTimeout(() => setVisible(false), FADE_MS);
      }, HOLD_MS);
    };

    frame = window.requestAnimationFrame(tick);

    return () => {
      window.cancelAnimationFrame(frame);
      window.clearTimeout(holdTimer);
      window.clearTimeout(doneTimer);
    };
  }, []);

  if (!visible) return null;

  return (
    <output
      aria-label="Loading BAML"
      aria-live="polite"
      style={{
        alignItems: 'center',
        background: '#FBF7ED',
        color: '#6D28D9',
        display: 'flex',
        inset: 0,
        justifyContent: 'center',
        opacity: leaving ? 0 : 1,
        pointerEvents: leaving ? 'none' : 'auto',
        position: 'fixed',
        transition: `opacity ${FADE_MS}ms cubic-bezier(0.4,0,0.2,1)`,
        zIndex: 100_000,
      }}
    >
      <pre
        style={{
          fontFamily:
            'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          fontSize: 'clamp(5px, 1vw, 14px)',
          fontWeight: 600,
          lineHeight: 1,
          margin: 0,
          maxWidth: '92vw',
          overflow: 'hidden',
          whiteSpace: 'pre',
        }}
      >
        {shownChars === null ? '' : BAML_ASCII.slice(0, shownChars)}
      </pre>
    </output>
  );
}
