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
        <h2 className="text-3xl md:text-4xl font-medium tracking-tighter text-center text-balance">
          People love BAML
        </h2>
        <p className="text-muted-foreground text-center text-balance font-medium">
          Code that agents write. Software that humans trust.
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
