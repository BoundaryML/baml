import { Github } from 'lucide-react';
import { siteConfig } from '@/app/_lib/config';
import { SectionHeader } from './section-header';
import { SocialProofTestimonials } from './testimonial-scroll';
import { Button } from './ui/button';

export function TestimonialSection() {
  const { testimonials } = siteConfig;

  return (
    <section
      className="flex flex-col items-center justify-center w-full"
      id="testimonials"
    >
      <SectionHeader>
        <h2
          className="text-center text-balance"
          style={{
            color: '#1A1612',
            fontSize: 'clamp(2rem, 4.5vw, 3.5rem)',
            fontWeight: 600,
            letterSpacing: '-0.03em',
            lineHeight: 1.02,
            margin: 0,
          }}
        >
          People love BAML
          <span style={{ color: '#6D28D9' }}>.</span>
        </h2>
        <p
          className="text-center text-balance"
          style={{
            color: '#5C5852',
            fontFamily: 'var(--font-serif), "Instrument Serif", Georgia, serif',
            fontSize: 18,
            fontStyle: 'italic',
            margin: '16px 0 0',
          }}
        >
          Agents write it. Teams ship with it.
        </p>
      </SectionHeader>
      <SocialProofTestimonials testimonials={testimonials} />
      <div className="mt-8 flex justify-center">
        <a
          href="https://github.com/BoundaryML/site/issues/new?template=testimonial.yml"
          rel="noopener noreferrer"
          target="_blank"
          className="editorial-btn"
        >
          <Github className="h-4 w-4" />
          Share your story
        </a>
      </div>
      <style>{`
        .editorial-btn {
          display: inline-flex;
          align-items: center;
          gap: 10px;
          padding: 12px 22px;
          border-radius: 8px;
          background: #ffffff;
          color: #1A1612;
          border: 1px solid #D9D3C4;
          font-size: 14px;
          font-weight: 500;
          letter-spacing: 0.01em;
          text-decoration: none;
          transition: background-color 200ms ease, border-color 200ms ease, color 200ms ease, transform 200ms ease;
        }
        .editorial-btn:hover {
          background: #FBF8F1;
          border-color: #6D28D9;
          color: #6D28D9;
          transform: translateY(-1px);
        }
      `}</style>
    </section>
  );
}
