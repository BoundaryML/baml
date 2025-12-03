import type { PromptInfo, TestCaseMetadata } from '../../../../sdk/interface';
import { CollapsibleMessage } from './collapsible-message';

export const RenderPrompt: React.FC<{
  prompt?: PromptInfo;
  testCase?: TestCaseMetadata;
}> = ({ prompt, testCase }) => {
  const chat = prompt?.type === 'chat' ? (prompt.messages ?? []) : [];

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
