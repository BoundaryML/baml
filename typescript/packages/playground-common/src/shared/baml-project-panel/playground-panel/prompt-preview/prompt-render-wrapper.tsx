import { useAtomValue } from 'jotai';
import { displaySettingsAtom } from '../preview-toolbar';
// import { PromptPreviewCurl } from './prompt-preview-curl'
import { PromptPreviewContent } from './prompt-preview-content';

export const PromptRenderWrapper = () => {
  const displaySettings = useAtomValue(displaySettingsAtom);

  // if (displaySettings === 'curl') {
  // return <PromptPreviewCurl />
  // }

  return <PromptPreviewContent />;
};
