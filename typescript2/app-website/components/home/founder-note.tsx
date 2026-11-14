import { SIGNUP } from './links';
import { Button, LINKS } from './ui';

// The founder's email, with the subject and the start of the message already filled in.
const FOUNDER_EMAIL = 'vaibhav@boundaryml.com';
const EMAIL_ME = `mailto:${FOUNDER_EMAIL}?subject=${encodeURIComponent(
  'Hi, I’m curious about Boundary/BAML',
)}&body=${encodeURIComponent(
  'Hi Vaibhav,\n\nI’m working on \n\nand I’d love to talk about \n',
)}`;

export function FounderNote({
  showTeamLink = false,
}: {
  showTeamLink?: boolean;
}) {
  return (
    <div className="mt-8 flex max-w-[44rem] flex-col gap-5 rounded-xl border border-rule bg-raised p-6 sm:flex-row sm:items-start">
      {/* biome-ignore lint/performance/noImgElement: a small static photo */}
      <img
        alt="Vaibhav Gupta"
        className="size-16 shrink-0 rounded-full object-cover"
        height={64}
        src="/profile-vbv.jpeg"
        width={64}
      />
      <div>
        <p className="text-[16px] leading-relaxed text-head">
          I know how big a bet a new programming language is. We’re engineers
          too, and we’re happy to talk it through with you. Email me directly.
        </p>
        <p className="mt-2 text-[14px] text-muted">
          Vaibhav Gupta, co-founder and CEO
        </p>
        <div className="mt-5 flex flex-wrap gap-3">
          <Button href={EMAIL_ME}>{FOUNDER_EMAIL}</Button>
          <Button href={SIGNUP} variant="secondary">
            Get on a call
          </Button>
          <Button href={LINKS.discord} variant="secondary">
            Ask on Discord
          </Button>
        </div>
        {showTeamLink && (
          <a
            className="mt-4 inline-block text-[14px] text-ink-2 underline decoration-white/30 underline-offset-2 hover:text-head"
            href="/who-are-we"
          >
            Meet the team →
          </a>
        )}
      </div>
    </div>
  );
}
