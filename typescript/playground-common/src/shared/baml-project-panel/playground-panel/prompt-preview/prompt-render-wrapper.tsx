import { useAtomValue } from 'jotai'
import { renderModeAtom } from '../preview-toolbar'
import { PromptPreviewContent } from './prompt-preview-content'
import { PromptPreviewCurl } from './prompt-preview-curl'

export const PromptRenderWrapper = () => {
  const renderMode = useAtomValue(renderModeAtom)

  if (renderMode === 'curl') {
    return <PromptPreviewCurl />
  }

  return <PromptPreviewContent />
}
