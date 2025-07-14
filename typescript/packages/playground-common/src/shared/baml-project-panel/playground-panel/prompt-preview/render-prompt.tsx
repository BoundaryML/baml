import type { WasmPrompt, WasmTestCase } from '@gloo-ai/baml-schema-wasm-web';
import { CollapsibleMessage } from './collapsible-message';

export const RenderPrompt: React.FC<{
  prompt: WasmPrompt;
  testCase?: WasmTestCase;
}> = ({ prompt, testCase }) => {
  const chat = prompt.as_chat() ?? [];

  return (
    <div className="flex flex-col gap-y-4">
      {chat.map((p, partIndex) => (
        <CollapsibleMessage
          key={`${partIndex}-${p.role}`}
          part={p}
          partIndex={partIndex}
          testCase={testCase}
        />
      ))}
    </div>
  );
};
