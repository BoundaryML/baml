/** biome-ignore-all lint/performance/noImgElement: 1.1em static local svg */
'use client';

import { useEffect, useState } from 'react';

// A floating "Try BAML" that rides along while you read, instead of install
// blocks interrupting the page. It shows once you're past the hero and hides as
// soon as the real install unit enters the viewport, so it never covers or
// competes with the thing it points at.
export function TryBamlFab() {
  const [on, setOn] = useState(false);

  useEffect(() => {
    const update = () => {
      const target = document.getElementById('try-baml');
      const targetInView = target
        ? target.getBoundingClientRect().top < window.innerHeight
        : false;
      setOn(window.scrollY > 700 && !targetInView);
    };

    update();
    window.addEventListener('scroll', update, { passive: true });
    window.addEventListener('resize', update, { passive: true });
    return () => {
      window.removeEventListener('scroll', update);
      window.removeEventListener('resize', update);
    };
  }, []);

  return (
    <a
      className={`wib-fab${on ? ' is-on' : ''}`}
      href="#try-baml"
      tabIndex={on ? undefined : -1}
    >
      <img alt="" src="/bamllogopurple.svg" />
      Try BAML
    </a>
  );
}
