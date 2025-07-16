import type {
  WasmChatMessagePart,
  WasmTestCase,
} from '@gloo-ai/baml-schema-wasm-web';
import { extractStringValues, getHighlightChunks } from './highlight-utils';
import { RenderPromptPart } from './render-text';
import { WebviewMedia } from './webview-media';

export const RenderPart: React.FC<{
  part: WasmChatMessagePart;
  testCase?: WasmTestCase;
  maxTextLength?: number;
}> = ({ part, testCase, maxTextLength = 20000 }) => {
  if (part.is_text()) {
    // Get the raw text without HTML encoding - React will handle escaping automatically
    const text = part.as_text() ?? '';
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

  const media = part.as_media();
  if (!media) {
    return null;
  }

  if (part.is_image()) {
    return <WebviewMedia bamlMediaType="image" media={media} />;
  }

  if (part.is_audio()) {
    return <WebviewMedia bamlMediaType="audio" media={media} />;
  }

  if (part.is_pdf()) {
    return <WebviewMedia bamlMediaType="pdf" media={media} />;
  }

  if (part.is_video()) {
    return <WebviewMedia bamlMediaType="video" media={media} />;
  }

  return null;
};
