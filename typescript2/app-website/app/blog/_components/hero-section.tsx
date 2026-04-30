import { CategoryFilter } from './category-filter';
import { NewsletterForm } from './newsletter-form';

interface HeroSectionProps {
  selectedCategory: string;
  onCategoryChange: (category: string) => void;
}

const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';

export function HeroSection({
  selectedCategory,
  onCategoryChange,
}: HeroSectionProps) {
  return (
    <section
      style={{
        width: '100%',
        padding: '96px 48px 64px',
        borderBottom: `1px solid ${BORDER}`,
      }}
    >
      <div style={{ maxWidth: 1200, margin: '0 auto' }}>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'minmax(0, 1.4fr) minmax(0, 1fr)',
            gap: 64,
            alignItems: 'start',
          }}
          className="blog-hero-grid"
        >
          {/* Left: title + newsletter */}
          <div>
            <p
              style={{
                fontSize: 13,
                fontWeight: 500,
                letterSpacing: '0.12em',
                textTransform: 'uppercase',
                color: EYEBROW,
                margin: 0,
              }}
            >
              Notes from the team
            </p>
            <h1
              style={{
                fontSize: 'clamp(2.5rem, 5vw, 4rem)',
                fontWeight: 600,
                lineHeight: 1.02,
                letterSpacing: '-0.03em',
                margin: '20px 0 0',
                color: INK,
              }}
            >
              BAML{' '}
              <span
                style={{
                  fontFamily: 'var(--font-serif)',
                  fontStyle: 'italic',
                  fontWeight: 500,
                  color: ACCENT,
                }}
              >
                thoughts
              </span>
              .
            </h1>
            <p
              style={{
                fontSize: 18,
                lineHeight: 1.6,
                color: MUTED,
                margin: '24px 0 0',
                maxWidth: 540,
              }}
            >
              Insights, tutorials, and updates from the BAML team. Stay ahead
              with the latest in AI development.
            </p>

            <div style={{ marginTop: 36, maxWidth: 480 }}>
              <NewsletterForm />
              <p
                style={{
                  fontSize: 12,
                  letterSpacing: '0.04em',
                  color: EYEBROW,
                  margin: '12px 0 0',
                }}
              >
                Get the latest posts delivered to your inbox.
              </p>
            </div>
          </div>

          {/* Right: categories */}
          <div
            style={{
              borderLeft: `1px solid ${BORDER}`,
              paddingLeft: 32,
            }}
            className="blog-hero-side"
          >
            <p
              style={{
                fontSize: 12,
                fontWeight: 500,
                letterSpacing: '0.14em',
                textTransform: 'uppercase',
                color: EYEBROW,
                margin: 0,
              }}
            >
              Browse by category
            </p>
            <div style={{ marginTop: 20 }}>
              <CategoryFilter
                onCategoryChange={onCategoryChange}
                selectedCategory={selectedCategory}
              />
            </div>
          </div>
        </div>
      </div>

      <style>{`
        @media (max-width: 900px) {
          .blog-hero-grid { grid-template-columns: 1fr !important; gap: 40px !important; }
          .blog-hero-side { border-left: none !important; padding-left: 0 !important; border-top: 1px solid ${BORDER}; padding-top: 32px; }
        }
      `}</style>
    </section>
  );
}
