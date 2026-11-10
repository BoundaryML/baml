'use client';

import { useEffect, useState } from 'react';
import { section } from './analytics';

// A small link that floats at the bottom of the screen once the reader is past the hero, for anyone
// who's convinced and wants to know how to start without rewriting their code. It hides again once
// the adoption section or the closing call to action is on screen, since those answer it.

export default function SeenEnough() {
  const [show, setShow] = useState(false);

  useEffect(() => {
    const ends = ['adopt', 'book']
      .map((id) => document.getElementById(id)?.closest('section'))
      .filter((el): el is HTMLElement => !!el);
    let past = false;
    let atEnd = false;
    const update = () => setShow(past && !atEnd);
    const onScroll = () => {
      past = window.scrollY > window.innerHeight;
      update();
    };
    const io = new IntersectionObserver((entries) => {
      atEnd =
        entries.some((e) => e.isIntersecting) ||
        ends.some((el) => el.getBoundingClientRect().top < window.innerHeight);
      update();
    });
    for (const el of ends) io.observe(el);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      window.removeEventListener('scroll', onScroll);
      io.disconnect();
    };
  }, []);

  return (
    <div
      {...section('seen-enough')}
      className="pointer-events-none fixed inset-x-0 bottom-5 z-40 flex justify-center px-4 transition-[opacity,translate] duration-300"
      // Hidden, it's out of the tab order and the accessibility tree, not only invisible.
      inert={!show}
      style={{ opacity: show ? 1 : 0, translate: show ? '0 0' : '0 12px' }}
    >
      <a
        className="focus-ring pointer-events-auto flex items-center gap-2 whitespace-nowrap rounded-full border border-clover/50 bg-ground/90 px-4 py-2.5 text-[14px] text-head shadow-lg backdrop-blur md:text-[15px]"
        href="#adopt"
        tabIndex={show ? 0 : -1}
      >
        <span className="text-clover">Seen enough?</span>
        <span className="sm:hidden">Start here</span>
        <span className="hidden sm:inline">
          Start in your existing codebase
        </span>
        <span aria-hidden>→</span>
      </a>
    </div>
  );
}
