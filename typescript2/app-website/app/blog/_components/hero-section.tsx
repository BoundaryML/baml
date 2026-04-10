import { CategoryFilter } from './category-filter';
import { NewsletterForm } from './newsletter-form';

interface HeroSectionProps {
  selectedCategory: string;
  onCategoryChange: (category: string) => void;
}

export function HeroSection({
  selectedCategory,
  onCategoryChange,
}: HeroSectionProps) {
  return (
    <section className="relative w-full py-12 px-6 md:p-16 lg:py-24">
      <div className="mx-auto max-w-7xl">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-12 lg:gap-16 items-start">
          {/* Left side - Title and description */}
          <div className="space-y-6">
            <div className="space-y-4">
              <h1 className="bg-gradient-to-br dark:from-white from-black from-30% dark:to-white/40 to-black/40 bg-clip-text py-4 sm:py-6 text-5xl font-medium leading-none tracking-tighter text-transparent text-balance sm:text-5xl md:text-6xl lg:text-6xl xl:text-7xl translate-y-[-1rem] animate-fade-in opacity-0 [--animation-delay:200ms]">
                BAML Thoughts
              </h1>
              <p className="text-xl text-muted-foreground max-w-lg">
                Insights, tutorials, and updates from the BAML team. Stay ahead
                with the latest in AI development.
              </p>
            </div>

            {/* Newsletter subscription */}
            <div className="space-y-4 pt-4">
              {/* <h3 className="text-lg font-semibold">Stay in the loop</h3> */}
              <div className="max-w-md">
                <NewsletterForm />
              </div>
              <p className="text-sm text-muted-foreground">
                Get the latest posts delivered to your inbox
              </p>
            </div>
          </div>

          {/* Right side - Category filters */}
          <div className="space-y-6">
            <div className="space-y-4">
              <h3 className="text-lg font-semibold">Browse by category</h3>
              <CategoryFilter
                onCategoryChange={onCategoryChange}
                selectedCategory={selectedCategory}
              />
            </div>

            {/* Quick stats or featured content */}
            {/* <div className="bg-muted/50 rounded-lg p-6 space-y-4">
              <h4 className="font-medium">Latest from BAML</h4>
              <div className="space-y-3 text-sm">
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Total posts</span>
                  <span className="font-medium">50+</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Categories</span>
                  <span className="font-medium">6</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-muted-foreground">Monthly readers</span>
                  <span className="font-medium">10K+</span>
                </div>
              </div>
            </div> */}
          </div>
        </div>
      </div>
    </section>
  );
}
