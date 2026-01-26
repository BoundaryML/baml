import type {
  ChatMessagePart,
  TestCaseMetadata,
} from '../../../../sdk/interface';
import { extractStringValues, getHighlightChunks } from './highlight-utils';
import { RenderPromptPart } from './render-text';
import { WebviewMedia } from './webview-media';

export const RenderPart: React.FC<{
  part: ChatMessagePart;
  testCase?: TestCaseMetadata;
  maxTextLength?: number;
}> = ({ part, testCase, maxTextLength = 20000 }) => {
  if (part.type === 'text') {
    // Get the raw text without HTML encoding - React will handle escaping automatically
    const text = part.content;
    // Skip processing if any input value is too large
    const hasLargeInput = (testCase?.inputs ?? []).some(
      (input) =>
        typeof input.value === 'string' && input.value.length > maxTextLength,
    );

    const allChunks = hasLargeInput
      ? []
      : extractStringValues(testCase?.inputs ?? []);

    const highlightChunks = getHighlightChunks(text, allChunks);

    return text ? (
      <RenderPromptPart text={text} highlightChunks={highlightChunks} />
    ) : null;
  }

  // For media parts, the content is the media data
  if (part.type === 'image') {
    return <WebviewMedia bamlMediaType="image" media={{ content: part.content }} />;
  }

  if (part.type === 'audio') {
    return <WebviewMedia bamlMediaType="audio" media={{ content: part.content }} />;
  }

  if (part.type === 'pdf') {
    return <WebviewMedia bamlMediaType="pdf" media={{ content: part.content }} />;
  }

  if (part.type === 'video') {
    return <WebviewMedia bamlMediaType="video" media={{ content: part.content }} />;
  }

  return null;
};
