// The save slots, as a game shows them.

import { useState } from 'react';
import type { Save, Slot } from './saves';

export function Home({
  slots,
  ready,
  onNew,
  onResume,
  onExport,
  onReset,
}: {
  slots: Slot[];
  /** Whether the formatter has loaded; every case is laid out by it. */
  ready: boolean;
  onNew: (slot: number) => void;
  onResume: (slot: number, save: Save) => void;
  onExport: (save: Save) => void;
  onReset: (slot: number) => void;
}) {
  const cards = slots.map((held, slot) => ({ held, slot }));
  return (
    <section className="home">
      <p className="blurb">
        A sitting is saved after every answer, so leave and come back whenever.
        {!ready && ' Loading the formatter…'}
      </p>
      {cards.map((card) => (
        <SlotCard
          held={card.held}
          key={card.slot}
          onExport={onExport}
          onNew={onNew}
          onReset={onReset}
          onResume={onResume}
          ready={ready}
          slot={card.slot}
        />
      ))}
    </section>
  );
}

function describe(save: Save): string {
  const kind = save.knobs.practice
    ? `practice, ${save.knobs.budget} cases`
    : 'until mastered';
  const reasons = save.full ? ', with reasons' : '';
  return `${save.answers.length} answered · ${kind}${reasons} · ${new Date(save.updated).toLocaleString()}`;
}

function SlotCard({
  slot,
  held,
  ready,
  onNew,
  onResume,
  onExport,
  onReset,
}: {
  slot: number;
  held: Slot;
  ready: boolean;
  onNew: (slot: number) => void;
  onResume: (slot: number, save: Save) => void;
  onExport: (save: Save) => void;
  onReset: (slot: number) => void;
}) {
  // Resetting takes two clicks: the second button only appears after the
  // first, so a slot cannot be lost to one stray click.
  const [armed, setArmed] = useState(false);
  const title = `Slot ${slot + 1}`;
  const reset = armed ? (
    <>
      <button
        className="danger"
        onClick={() => {
          setArmed(false);
          onReset(slot);
        }}
        type="button"
      >
        Really reset
      </button>
      <button className="quiet" onClick={() => setArmed(false)} type="button">
        Keep
      </button>
    </>
  ) : (
    <button className="quiet" onClick={() => setArmed(true)} type="button">
      Reset
    </button>
  );
  switch (held.kind) {
    case 'empty':
      return (
        <div className="slot">
          <h3>{title}</h3>
          <p>Empty.</p>
          <div className="choices">
            <button disabled={!ready} onClick={() => onNew(slot)} type="button">
              New sitting
            </button>
          </div>
        </div>
      );
    case 'held':
      return (
        <div className="slot">
          <h3>{title}</h3>
          <p>{describe(held.save)}</p>
          <div className="choices">
            <button
              disabled={!ready}
              onClick={() => onResume(slot, held.save)}
              type="button"
            >
              Continue
            </button>
            <button
              className="quiet"
              onClick={() => onExport(held.save)}
              type="button"
            >
              Export
            </button>
            {reset}
          </div>
        </div>
      );
    case 'unreadable':
      return (
        <div className="slot">
          <h3>{title}</h3>
          <p>Unreadable: {held.reason}.</p>
          <div className="choices">{reset}</div>
        </div>
      );
  }
}
