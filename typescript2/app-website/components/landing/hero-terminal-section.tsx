import {
  AnimatedSpan,
  Terminal,
  TypingAnimation,
} from '@/components/magicui/terminal';

// import {
//   Table,
//   TableBody,
//   TableCell,
//   TableHead,
//   TableHeader,
//   TableRow,
// } from '@/components/ui/table';
export function HeroTerminalSection() {
  return (
    <Terminal className="w-full max-w-full min-h-[250px] font-mono">
      <TypingAnimation delay={1000}>$ baml-cli generate</TypingAnimation>
      <AnimatedSpan delay={2500}>
        <span className="text-muted-foreground font-mono">
          ✅ Generated Python Client
        </span>
      </AnimatedSpan>
      <AnimatedSpan delay={2900}>
        <span className="text-muted-foreground font-mono">
          ✅ Generated TypeScript Client
        </span>
      </AnimatedSpan>
      <AnimatedSpan delay={3300}>
        <span className="text-muted-foreground font-mono">
          ✅ Generated Go Client
        </span>
      </AnimatedSpan>
      <AnimatedSpan delay={3700}>
        <span className="text-muted-foreground font-mono">
          ✅ Generated Ruby Client
        </span>
      </AnimatedSpan>
    </Terminal>
  );
}
