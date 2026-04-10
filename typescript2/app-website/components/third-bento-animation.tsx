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
export function ThirdBentoAnimation() {
  return (
    <div className="w-full max-w-full h-full font-mono p-4">
      <Terminal className="w-full min-h-[250px] font-mono shadow-lg">
        <TypingAnimation delay={1000}>$ baml-cli test</TypingAnimation>
        <AnimatedSpan delay={2500}>
          <span className="text-muted-foreground font-mono">
            ✅ Testing ResumeParser...
          </span>
        </AnimatedSpan>
        <AnimatedSpan delay={2900}>
          <span className="text-muted-foreground font-mono">
            ✅ Testing SentimentAnalyzer...
          </span>
        </AnimatedSpan>
        <AnimatedSpan delay={3300}>
          <span className="text-muted-foreground font-mono">
            ✅ Testing CodeReviewer...
          </span>
        </AnimatedSpan>
        <AnimatedSpan delay={3700}>
          <span className="text-muted-foreground font-mono">
            ✅ All Agents tested successfully
          </span>
        </AnimatedSpan>
      </Terminal>
    </div>
  );
}
