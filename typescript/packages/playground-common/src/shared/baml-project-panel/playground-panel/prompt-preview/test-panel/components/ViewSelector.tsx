import * as React from 'react'
import { useAtom } from 'jotai'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@baml/ui/select'
import { TestPanelViewType, testPanelViewTypeAtom } from './atoms'

export const ViewSelector = () => {
  const [viewType, setViewType] = useAtom(testPanelViewTypeAtom)

  return (
    <Select value={viewType} onValueChange={(value) => setViewType(value as TestPanelViewType)}>
      <SelectTrigger>
        <SelectValue placeholder='Select view' />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value={TestPanelViewType.TABULAR}>Table View</SelectItem>
        <SelectItem value={TestPanelViewType.CARD_EXPANDED}>Detailed View</SelectItem>
        {/* <SelectItem value={TestPanelViewType.CLIENT_GRAPH}>Client Graph</SelectItem> */}
        {/* <SelectItem value={TestPanelViewType.CARD_SIMPLE}>
          Card View (Simple)
        </SelectItem> */}
      </SelectContent>
    </Select>
  )
}
