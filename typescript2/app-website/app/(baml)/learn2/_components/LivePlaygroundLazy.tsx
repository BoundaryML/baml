'use client';

import dynamic from 'next/dynamic';

/**
 * Client-only boundary for the live playground. The editor + BexVM worker +
 * `@b/pkg-playground` are browser-only, so we load them with `ssr: false`
 * (which is why this wrapper is a Client Component — `ssr:false` can't live in
 * a Server Component).
 */
const LivePlayground = dynamic(() => import('./LivePlayground'), {
  ssr: false,
  loading: () => (
    <div className="baml-playground-root l2-live">
      <div className="l2-live-loading">loading playground…</div>
    </div>
  ),
});

export default LivePlayground;
