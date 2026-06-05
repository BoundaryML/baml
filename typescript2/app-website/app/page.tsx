import { AsciiBootLoader } from '@/components/ascii-boot-loader';
import { FooterSection } from '@/components/footer-section';
import { ForceLightTheme } from '@/components/force-light-theme';
import { IncrementalAdoption } from '@/components/landing/incremental-adoption';
import { LanguageFanout } from '@/components/landing/language-fanout';
import { PerspectiveSlider } from '@/components/landing/perspective-slider';
import { VariantHome } from '@/components/landing/variant-home';
import { WhyALanguage } from '@/components/landing/why-a-language';
import { TestimonialSection } from '@/components/testimonial-section';

export default async function Page() {
  return (
    <div className="w-full bg-background text-foreground">
      <AsciiBootLoader />
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

      {/* Same program, two views — code ↔ graph */}
      <PerspectiveSlider />

      {/* One file → every language fanout */}
      <LanguageFanout />

      {/* Built for the Future of AI — bento grid in editorial theme */}
      {/* <BuiltForTheFuture /> */}

      {/* Divider between language fanout and testimonials */}
      <div
        aria-hidden
        style={{
          borderTop: '1px solid #D9D3C4',
          margin: '0 auto',
          maxWidth: '100rem',
          width: '100%',
        }}
      />

      <div className="relative z-10 mx-auto flex w-full max-w-[100rem] flex-col items-center gap-12 bg-background sm:gap-20">
        <TestimonialSection />
        <FooterSection />
      </div>
    </div>
  );
}
