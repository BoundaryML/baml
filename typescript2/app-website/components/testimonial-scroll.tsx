import Image from 'next/image';
import { cn } from '@/lib/utils';
import Marquee from './magicui/marquee';

export interface TestimonialCardProps
  extends React.HTMLAttributes<HTMLDivElement> {
  name: string;
  role: string;
  img?: string;
  description: React.ReactNode;
  className?: string;
}

export const TestimonialCard = ({
  description,
  name,
  img,
  role,
  className,
  ...props
}: TestimonialCardProps) => (
  <div
    className={cn(
      'flex w-full break-inside-avoid flex-col items-center justify-between gap-6 rounded-xl p-4',
      'bg-transparent border border-border/50',
      className,
    )}
    {...props}
  >
    <div className="select-none leading-relaxed font-normal text-foreground/70">
      {description}
    </div>

    <div className="flex w-full select-none items-center justify-start gap-3.5">
      {img && (
        <Image
          alt={name}
          className="size-8 rounded-full"
          height={32}
          sizes="32px"
          src={img}
          width={32}
        />
      )}

      <div>
        <p className="font-medium text-foreground">{name}</p>
        <p className="text-xs font-normal text-muted-foreground">{role}</p>
      </div>
    </div>
  </div>
);

interface Testimonial {
  id: string;
  name: string;
  role: string;
  img: string;
  description: React.ReactNode;
}

export function SocialProofTestimonials({
  testimonials,
}: {
  testimonials: Testimonial[];
}) {
  const COLS = 3;
  const perCol = Math.max(1, Math.ceil(testimonials.length / COLS));
  const columns = Array.from({ length: COLS }, (_, i) =>
    Array.from(
      { length: perCol },
      (_unused, j) => testimonials[(i * perCol + j) % testimonials.length],
    ),
  );

  return (
    <div className="h-full">
      <div className="px-10">
        <div className="relative max-h-[750px] overflow-hidden">
          <div className="grid gap-6 md:grid-cols-2 xl:grid-cols-3">
            {columns.map((col, i) => (
              <Marquee
                className={cn({
                  '[--duration:8s]': i === 1,
                  '[--duration:10s]': i === 0,
                  '[--duration:12s]': i === 2,
                })}
                // biome-ignore lint/suspicious/noArrayIndexKey: column index is stable for the lifetime of the component
                key={i}
                vertical
              >
                {col.map((card, j) => (
                  <TestimonialCard {...card} key={`${i}-${card.id}-${j}`} />
                ))}
              </Marquee>
            ))}
          </div>
          <div className="pointer-events-none absolute inset-x-0 bottom-0 h-1/6 md:h-1/5 w-full bg-gradient-to-t from-background from-20%" />
          <div className="pointer-events-none absolute inset-x-0 top-0 h-1/6 md:h-1/5 w-full bg-gradient-to-b from-background from-20%" />
        </div>
      </div>
    </div>
  );
}
