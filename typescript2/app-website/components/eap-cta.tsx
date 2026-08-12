import type { ReactNode } from 'react';
import styles from './cta.module.css';

// Early Access CTA band. A peer to the Discord CTA, but the primary (accent)
// one: book a live onboarding session with the team.

export function EapCta({
  title = 'Get onboarded, live',
  children = "Book a free 45-minute session with the team. We'll get you set up and work through your actual use case.",
}: {
  title?: string;
  children?: ReactNode;
}) {
  return (
    <section className={styles.band}>
      <div className={styles.head}>
        <svg
          aria-hidden="true"
          className={`${styles.ico} ${styles.icoAccent}`}
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={1.7}
          viewBox="0 0 24 24"
        >
          <path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z" />
          <path d="M12 15l-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z" />
          <path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0" />
          <path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5" />
        </svg>
        <div className={styles.title}>{title}</div>
      </div>
      <p className={styles.sub}>{children}</p>
      <a className={styles.btnPrimary} href="/eap">
        <svg
          aria-hidden="true"
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={2}
          viewBox="0 0 24 24"
        >
          <path d="M5 12h14M13 6l6 6-6 6" />
        </svg>
        Book a session
      </a>
    </section>
  );
}
