'use client'

import { Provider, createStore } from 'jotai'
import { createJSONStorage } from 'jotai/utils'
import type { SyncStorage } from 'jotai/vanilla/utils/atomWithStorage'
import { DevTools } from 'jotai-devtools'
import 'jotai-devtools/styles.css'

export const atomStore = createStore()
export const sessionStore: SyncStorage<any> = createJSONStorage(() => sessionStorage)

export default function JotaiProvider({ children }: { children: React.ReactNode }) {
  return (
    <Provider store={atomStore}>
      <DevTools store={atomStore} />
      {children}
    </Provider>
  )
}
