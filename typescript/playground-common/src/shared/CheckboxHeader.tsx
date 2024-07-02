import { useAppState } from './AppStateContext'
import { Checkbox } from '../components/ui/checkbox'
import { useAtomValue } from 'jotai'
import { selectedTestCaseAtom } from '../baml_wasm_web/EventListener'
import Link from './Link'
import { ShowSettingsButton } from './SettingsDialog'

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
  const { showTokens, setShowTokens, showWhitespace, setShowWhitespace, showCurlRequest, setShowCurl } = useAppState()

  return (
    <div className='flex flex-wrap justify-start gap-4 px-2 py-2 text-xs whitespace-nowrap'>
      <PromptCheckbox checked={showTokens} onChange={setShowTokens}>
        Show Tokens
      </PromptCheckbox>
      <PromptCheckbox checked={showWhitespace} onChange={setShowWhitespace}>
        Whitespace
      </PromptCheckbox>
      <PromptCheckbox checked={showCurlRequest} onChange={setShowCurl}>
        Raw cURL
      </PromptCheckbox>
      <ShowSettingsButton iconClassName='h-5' />
    </div>
  )
}
