'use client';

import type { ReactNode } from 'react';
import { siteConfig } from '@/app/_lib/config';

const BG = '#ffffff';
const INK = '#1A1612';
const MUTED = '#5C5852';
const KICKER = '#8A8580';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const DASH = 'rgba(109, 40, 217, 0.35)';

type Item = {
  id: number;
  title: string;
  description: string;
  content: ReactNode;
};

export function BuiltForTheFuture() {
  const { title, description, items } = siteConfig.bentoSection;

  return (
    <section
      className="w-full"
      aria-labelledby="built-for-the-future-heading"
      style={{ backgroundColor: BG }}
    >
      <style>{`
        @media (max-width: 900px) {
          .bff-grid { grid-template-columns: 1fr !important; }
          .bff-cell-v-divider { display: none !important; }
          .bff-cell:nth-child(odd) .bff-cell-h-divider { display: block !important; }
          .bff-cell:first-child .bff-cell-h-divider { display: none !important; }
        }
      `}</style>

      <div
        className="mx-auto"
        style={{
          maxWidth: '1600px',
          borderLeft: `1px solid ${BORDER}`,
          borderRight: `1px solid ${BORDER}`,
          borderBottom: `1px solid ${BORDER}`,
          padding: '120px 48px',
        }}
      >
        <div className="mx-auto flex w-full max-w-[900px] flex-col items-center text-center">
          <p
            style={{
              fontSize: '13px',
              fontWeight: 500,
              letterSpacing: '0.12em',
              textTransform: 'uppercase',
              color: KICKER,
              margin: '0 0 20px',
            }}
          >
            Built for the future
          </p>

          <div
            aria-hidden="true"
            style={{
              width: 56,
              borderTop: `1px dashed ${DASH}`,
              margin: '0 0 28px',
            }}
          />

          <h2
            id="built-for-the-future-heading"
            style={{
              fontSize: 'clamp(2rem, 4.5vw, 3.5rem)',
              fontWeight: 600,
              lineHeight: 1.02,
              letterSpacing: '-0.03em',
              color: INK,
              margin: '0 0 28px',
            }}
          >
            {title}
            <span style={{ color: ACCENT }}>.</span>
          </h2>

          <p
            style={{
              fontSize: 'clamp(1rem, 1.25vw, 1.125rem)',
              lineHeight: 1.6,
              color: MUTED,
              maxWidth: 620,
              margin: 0,
            }}
          >
            {description}
          </p>
        </div>

        <div
          className="bff-grid mx-auto"
          style={{
            marginTop: 96,
            maxWidth: 1200,
            display: 'grid',
            gridTemplateColumns: '1fr 1fr',
          }}
        >
          {(items as Item[]).map((item, index) => {
            const isRightCol = index % 2 === 1;
            const isSecondRow = index >= 2;
            return (
              <div
                key={item.id}
                className="bff-cell"
                style={{
                  position: 'relative',
                  padding: '56px 40px 40px',
                  display: 'flex',
                  flexDirection: 'column',
                }}
              >
                {isRightCol && (
                  <div
                    aria-hidden="true"
                    className="bff-cell-v-divider"
                    style={{
                      position: 'absolute',
                      top: 24,
                      bottom: 24,
                      left: 0,
                      borderLeft: `1px dashed ${DASH}`,
                    }}
                  />
                )}
                {isSecondRow && (
                  <div
                    aria-hidden="true"
                    className="bff-cell-h-divider"
                    style={{
                      position: 'absolute',
                      left: 24,
                      right: 24,
                      top: 0,
                      borderTop: `1px dashed ${DASH}`,
                    }}
                  />
                )}

                <div
                  style={{
                    position: 'relative',
                    minHeight: 260,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    overflow: 'hidden',
                    marginBottom: 32,
                  }}
                >
                  {item.content}
                </div>

                <h3
                  style={{
                    fontSize: '1.375rem',
                    fontWeight: 600,
                    letterSpacing: '-0.015em',
                    lineHeight: 1.2,
                    color: INK,
                    margin: '0 0 10px',
                  }}
                >
                  {item.title}
                </h3>

                <p
                  style={{
                    fontSize: '0.975rem',
                    lineHeight: 1.55,
                    color: MUTED,
                    margin: 0,
                  }}
                >
                  {item.description}
                </p>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
