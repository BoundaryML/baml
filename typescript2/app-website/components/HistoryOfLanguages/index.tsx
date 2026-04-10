'use client';

import { HISTORY_LANGUAGE_PANELS } from './constants';
import styles from './styles.module.css';
import { TimelinePanel } from './TimelinePanel';
import { useIntersectionTrigger } from './useIntersectionTrigger';

function PanelSlot({ index }: { index: number }) {
  const panel = HISTORY_LANGUAGE_PANELS[index];
  const { hasTriggered, ref } = useIntersectionTrigger(0.2);

  return (
    <div
      ref={ref}
      className={styles.panelSlot}
      data-active={hasTriggered ? 'true' : 'false'}
    >
      <TimelinePanel panel={panel} active={hasTriggered} shouldRender={hasTriggered} />
    </div>
  );
}

export function HistoryOfLanguages() {
  return (
    <section id="story" className={styles.section} aria-label="The History of Languages">
      <div className={styles.intro}>
        <h2 className={styles.title}>The History of Languages</h2>
        <p className={styles.subtitle}>Every era gets the language it deserves.</p>
      </div>

      <div className={styles.track}>
        {HISTORY_LANGUAGE_PANELS.map((_, i) => (
          <PanelSlot key={HISTORY_LANGUAGE_PANELS[i].id} index={i} />
        ))}
      </div>
    </section>
  );
}
