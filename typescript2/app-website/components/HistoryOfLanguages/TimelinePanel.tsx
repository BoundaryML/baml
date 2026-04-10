'use client';

import { ParticleCanvas } from './ParticleCanvas';
import type { TimelinePanelDef } from './constants';
import styles from './styles.module.css';

type TimelinePanelProps = {
  panel: TimelinePanelDef;
  active: boolean;
  shouldRender: boolean;
};

export function TimelinePanel({ active, panel, shouldRender }: TimelinePanelProps) {
  return (
    <article className={styles.panel} data-active={active ? 'true' : 'false'}>
      <div className={styles.panelInner}>
        <p className={styles.transitionText}>{panel.transitionText}</p>
        <h3 className={styles.heading}>
          <span className={panel.isBaml ? styles.languageBaml : styles.language}>
            {panel.language}
          </span>
          <span className={styles.year}>{panel.year}</span>
        </h3>

        <div
          aria-label={`${panel.language} era landmark`}
          role="img"
          className={styles.asciiContainer}
        >
          <ParticleCanvas
            shapeId={panel.shapeId}
            active={shouldRender && active}
            isBaml={panel.isBaml}
          />
        </div>

        <p className={styles.tagline}>{panel.tagline}</p>
      </div>
    </article>
  );
}
