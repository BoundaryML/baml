'use client'

import { Provider, createStore } from 'jotai'
import { createJSONStorage } from 'jotai/utils'
import { SyncStorage } from 'jotai/vanilla/utils/atomWithStorage'

export const atomStore = createStore()
export const sessionStore: SyncStorage<any> = createJSONStorage(() => sessionStorage)

export default function JotaiProvider({ children }: { children: React.ReactNode }) {
  return <Provider store={atomStore}>{children}</Provider>
}
