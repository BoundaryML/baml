import type { WasmPrompt, WasmTestCase } from '@gloo-ai/baml-schema-wasm-web';
import { RenderPart } from './render-part';

export const RenderPrompt: React.FC<{
  prompt: WasmPrompt;
  testCase?: WasmTestCase;
}> = ({ prompt, testCase }) => {
  const chat = prompt.as_chat() ?? [];

  return (
    <div className="space-y-4 h-full">
      {chat.map((p, partIndex) => (
        <div
          key={partIndex}
          className={`border-l-4 pl-4 ${
            p.role === 'assistant'
              ? 'border-indigo-300'
              : p.role === 'user'
                ? 'border-fuchsia-300'
                : p.role === 'system'
                  ? 'border-amber-300'
                  : 'border-gray-300'
          }`}
        >
          <div className="text-xs text-muted-foreground mb-2">{p.role}</div>
          <div className="space-y-3">
            {p.parts.map((part, index) => (
              <RenderPart
                key={`${partIndex}-${index}`}
                part={part}
                testCase={testCase}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
};
