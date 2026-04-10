import { SyntaxTypingAnimation } from '@/components/magicui/syntax-typing-animation';
import { Terminal } from '@/components/magicui/terminal';

const typingText = `function AnalyzeCodebase(code: string) -> Analysis {
  client openai/gpt-4o
  prompt "Analyze the codebase for: {code}"
}

let result = b.AnalyzeCodebase("Find all auth endpoints")

{
  endpoints: ["/api/auth/login"],
  issues: 1,
  recommendations: ["Add rate limiting"]
}
`;

export function HeroTerminalSection() {
  return (
    <div className="w-full text-left">
      <Terminal
        className="w-full max-w-full min-h-[360px]"
        // filename="prompt.baml"
      >
        <SyntaxTypingAnimation
          code={typingText}
          delay={1000}
          duration={50}
          language="typescript"
        />
      </Terminal>
    </div>
  );
}
