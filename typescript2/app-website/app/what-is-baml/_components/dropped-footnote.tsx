'use client';

import { useSyncExternalStore } from 'react';
import {
  getBaml,
  getBamlServer,
  getStartServer,
  getWouldHave,
  subscribeDropped,
} from './dropped-store';

// The counter at the bottom never stops. Switching the widget above to BAML
// does not save these events, it just means they are no longer yours to lose.
export function DroppedFootnote() {
  const baml = useSyncExternalStore(subscribeDropped, getBaml, getBamlServer);
  const n = useSyncExternalStore(
    subscribeDropped,
    getWouldHave,
    getStartServer,
  );
  const count = n.toLocaleString('en-US');

  return (
    <p className="wib-footnote">
      {baml
        ? `OTEL would have dropped ${count} events while you read this page. You have all of them.`
        : `OTEL dropped ${count} events while you read this page.`}
    </p>
  );
}
