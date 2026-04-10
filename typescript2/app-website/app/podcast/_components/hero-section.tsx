import { Calendar } from 'lucide-react';
import Link from 'next/link';
import { Button } from '@/components/ui/button';

const podcastPlatforms = [
  { icon: '📅', name: 'Event Calendar', url: 'https://lu.ma/baml' },
  { icon: '💬', name: 'Discord', url: 'https://boundaryml.com/discord' },
  { icon: '🚀', name: 'GitHub', url: 'https://github.com/boundaryml/baml' },
  { icon: '📺', name: 'YouTube', url: 'https://www.youtube.com/@boundaryml' },
];

export function HeroSection() {
  return (
    <section className="relative w-full p-4 sm:p-8 lg:p-16 lg:py-24">
      <div className="mx-auto max-w-7xl">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-8 lg:gap-16 items-start">
          {/* Left side - Title and description */}
          <div className="space-y-6">
            <div className="space-y-4">
              <h1 className="bg-gradient-to-br dark:from-white from-black from-30% dark:to-white/40 to-black/40 bg-clip-text py-4 sm:py-6 text-5xl font-medium leading-none tracking-tighter text-transparent text-balance sm:text-5xl md:text-6xl lg:text-6xl xl:text-7xl translate-y-[-1rem] animate-fade-in opacity-0 [--animation-delay:200ms]">
                <span className="text-primary">🦄</span> ai that works
              </h1>
              <p className="text-lg sm:text-xl text-muted-foreground max-w-lg">
                A weekly conversation about how we can all get the most juice
                out of todays models with{' '}
                <Link
                  className="underline hover:text-foreground transition-colors"
                  href="https://www.github.com/hellovai"
                >
                  @hellovai
                </Link>{' '}
                &{' '}
                <Link
                  className="underline hover:text-foreground transition-colors"
                  href="https://www.github.com/dexhorthy"
                >
                  @dexhorthy
                </Link>
              </p>
            </div>

            {/* Schedule info */}
            <div className="space-y-4 pt-4">
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Calendar className="h-4 w-4" />
                <span>
                  Every{' '}
                  <span className="font-semibold text-foreground">Tuesday</span>{' '}
                  at{' '}
                  <span className="font-semibold text-foreground">
                    10 AM PST
                  </span>
                </span>
              </div>
              <p className="text-sm text-muted-foreground max-w-md">
                1 hour of live code, Q&A with some prepped content to help you
                take your AI app from a demo to production.
              </p>
            </div>
          </div>

          {/* Right side - Platform links and stats */}
          <div className="space-y-6">
            <div className="w-full overflow-hidden rounded-lg">
              <iframe
                allowFullScreen
                aria-hidden="false"
                frameBorder="0"
                height="250"
                src="https://lu.ma/embed/calendar/cal-NQYQhHfQN7sg4BF/events?lt=light"
                style={{
                  backgroundColor: 'transparent',
                  border: '1px solid #bfcbda88',
                  borderRadius: '4px',
                  width: '100%',
                }}
                title="Calendar"
              />
            </div>
            <div className="space-y-4">
              <h3 className="text-lg font-semibold">Join the conversation</h3>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                {podcastPlatforms.map((platform) => (
                  <Button
                    asChild
                    className="justify-start"
                    key={platform.name}
                    variant="outline"
                  >
                    <Link
                      className="flex items-center gap-2"
                      href={platform.url}
                    >
                      <span>{platform.icon}</span>
                      {platform.name}
                    </Link>
                  </Button>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
