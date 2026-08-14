import { Button } from '@/components/ui/button';

export function NewsletterCTA() {
  return (
    <section className="w-full px-4 py-20">
      <div className="mx-auto max-w-4xl text-center">
        <h2 className="text-3xl font-bold mb-4">Stay Updated</h2>
        <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
          Get the latest BAML tutorials, updates, and insights delivered to your
          inbox.
        </p>
        <div className="flex flex-col sm:flex-row gap-4 justify-center max-w-md mx-auto">
          <input
            className="flex-1 px-4 py-2 border border-border rounded-md bg-background"
            placeholder="Enter your email"
            type="email"
          />
          <Button>Subscribe</Button>
        </div>
      </div>
    </section>
  );
}
