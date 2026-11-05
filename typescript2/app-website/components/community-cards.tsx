import type { ReactNode } from 'react';
import { Button } from '@/components/home/ui';

// The early access and Discord cards on /quickstart and /eap, in the
// homepage's BAML lane.

const DISCORD_URL = 'https://boundaryml.com/discord';
const DISCORD_PATH =
  'M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z';

/** A card with an icon, a title, a line of text, and an action at the bottom. */
function Card({
  icon,
  title,
  children,
  action,
}: {
  icon: ReactNode;
  title: string;
  children: ReactNode;
  action: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-4 rounded-2xl border border-rule bg-raised p-5 md:p-6">
      <div className="flex items-center gap-3">
        <span className="size-6 shrink-0 text-clover">{icon}</span>
        <h2 className="font-display font-medium text-[20px] leading-[1.2] tracking-[-0.02em] text-head">
          {title}
        </h2>
      </div>
      <p className="text-[16px] leading-[1.5] text-body">{children}</p>
      <div className="mt-auto pt-1">{action}</div>
    </div>
  );
}

/** Book a live onboarding session. `primary` when it is the one main action in view. */
export function EapCard({
  variant = 'primary',
}: {
  variant?: 'primary' | 'secondary';
}) {
  return (
    <Card
      action={
        <Button href="/eap" variant={variant}>
          Book a session
        </Button>
      }
      icon={
        <svg
          aria-hidden="true"
          className="size-6"
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth={1.7}
          viewBox="0 0 24 24"
        >
          <path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z" />
          <path d="M12 15l-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z" />
          <path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0" />
          <path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5" />
        </svg>
      }
      title="Get onboarded, live"
    >
      Book a free 45-minute session with the team. We'll get you set up and work
      through your actual use case.
    </Card>
  );
}

/** Join the Discord. Always a secondary action. */
export function DiscordCard() {
  return (
    <Card
      action={
        <Button href={DISCORD_URL} variant="secondary">
          Join the Discord
        </Button>
      }
      icon={
        <svg
          aria-hidden="true"
          className="size-6"
          fill="currentColor"
          viewBox="0 0 24 24"
        >
          <path d={DISCORD_PATH} />
        </svg>
      }
      title="Questions or feedback?"
    >
      Join the BAML Discord. We're around to help you get set up, and we read
      every bit of feedback.
    </Card>
  );
}
