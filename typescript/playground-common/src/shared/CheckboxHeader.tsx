import { useAppState } from './AppStateContext'
import { Checkbox } from '../components/ui/checkbox'

const PromptCheckbox = ({
  children,
  checked,
  onChange,
}: {
  children: React.ReactNode
  checked: boolean
  onChange: (checked: boolean) => void
}) => {
  return (
    <div className='flex flex-row items-center gap-1'>
      <Checkbox checked={checked} onCheckedChange={onChange} className='border-vscode-descriptionForeground' />
      <span className='text-vscode-descriptionForeground'>{children}</span>
    </div>
  )
}

export const CheckboxHeader = () => {
  const {
    showTokens,
    setShowTokens,
    showWhitespace,
    setShowWhitespace,
    wrapText,
    setWrapText,
    showCurlRequest,
    setShowCurl,
  } = useAppState()

  return (
    <div className='flex flex-wrap justify-start gap-4 px-2 py-2 text-xs whitespace-nowrap'>
      <PromptCheckbox checked={showTokens} onChange={setShowTokens}>
        Show Tokens
      </PromptCheckbox>
      <PromptCheckbox checked={showWhitespace} onChange={setShowWhitespace}>
        Whitespace
      </PromptCheckbox>
      <PromptCheckbox checked={showCurlRequest} onChange={setShowCurl}>
        Raw Curl
      </PromptCheckbox>
    </div>
  )
}
