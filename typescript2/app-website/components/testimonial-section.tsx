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
          Typed prompt boundaries for structured-output LLM work.
        </p>
      </SectionHeader>
      <SocialProofTestimonials testimonials={testimonials} />
      <div className="mt-8 flex justify-center">
        <Button asChild className="gap-2" size="lg" variant="outline">
          <a
            href="https://github.com/BoundaryML/site/issues/new?template=testimonial.yml"
            rel="noopener noreferrer"
            target="_blank"
          >
            <Github className="h-4 w-4" />
            Share your story
          </a>
        </Button>
      </div>
    </section>
  );
}
