'use client'

import { Provider, createStore } from 'jotai'

export const atomStore = createStore()

export default function Providers({ children }: { children: React.ReactNode }) {
  return <Provider store={atomStore}>{children}</Provider>
}
