import { atom } from 'jotai'
import { atomWithStorage } from 'jotai/utils'

export enum TestPanelViewType {
  TABULAR = 'tabular',
  CARD_EXPANDED = 'card_expanded',
  CARD_SIMPLE = 'card_simple',
  CLIENT_GRAPH = 'client_graph',
}

export type ResponseViewType = 'parsed' | 'pretty' | 'raw'

export interface TabularViewConfig {
  showInputs: boolean
  showModel: boolean
  responseViewType: ResponseViewType
  showDuration: boolean
}

export const testPanelViewTypeAtom = atomWithStorage<TestPanelViewType>('testPanelViewType', TestPanelViewType.TABULAR)
export const tabularViewConfigAtom = atomWithStorage<TabularViewConfig>('tabularViewConfig', {
  showInputs: true,
  showModel: false,
  responseViewType: 'parsed',
  showDuration: false,
})
