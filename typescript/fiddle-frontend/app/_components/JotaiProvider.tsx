'use client'

import { Provider, createStore } from 'jotai'
import { createJSONStorage } from 'jotai/utils'

export const atomStore = createStore()
export const sessionStore = createJSONStorage(() => sessionStorage)

export default function JotaiProvider({ children }: { children: React.ReactNode }) {
  return <Provider store={atomStore}>{children}</Provider>
}
