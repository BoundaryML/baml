import { FooterSection } from '@/components/footer-section';
import { VariantHome } from '@/components/landing/variant-home';
import { IncrementalAdoption } from '@/components/landing/incremental-adoption';
import { StoryTimeline } from '@/components/landing/story-timeline';
import { WhyALanguage } from '@/components/landing/why-a-language';
import { BuiltForTheFuture } from '@/components/landing/built-for-the-future';
import { ScrollVideo } from '@/components/landing/scroll-video';
import { TestimonialSection } from '@/components/testimonial-section';
import { ForceLightTheme } from '@/components/force-light-theme';

export default async function Page() {
  return (
    <div className="w-full bg-background text-foreground">
      <ForceLightTheme />
      {/* Variant UI shell (Nav + Hero + Statement + Exhibit) */}
      <VariantHome />

      {/* Scroll-scrubbed intro video — full length tied to scroll */}
      {/* <ScrollVideo src="/scroll-animation.mp4" /> */}

      {/* <section
        className="w-full relative"
        id="story"
        aria-label="A brief history of languages"
      >
        <StoryTimeline />
      </section> */}

      {/* Why a Language — typographic two-column argument */}
      <WhyALanguage />

      {/* Incremental adoption — sticky scroll explainer */}
      <IncrementalAdoption />

      {/* Built for the Future of AI — bento grid in editorial theme */}
      {/* <BuiltForTheFuture /> */}

      <div className="relative z-10 mx-auto flex w-full max-w-[100rem] flex-col items-center gap-12 border-x border-border bg-background sm:gap-20">
        <TestimonialSection />
        <FooterSection />
      </div>
    </div>
  );
}
