import { atom, useAtom } from 'jotai'
import { atomWithStorage, createJSONStorage } from 'jotai/utils'

const storage = createJSONStorage(() => sessionStorage)

export const availableProjectsStorageAtom = atomWithStorage<string[]>('available-projects', [])

export const selectedProjectStorageAtom = atomWithStorage<string | null>('selected-project', null)

export const selectedFunctionStorageAtom = atomWithStorage<string | null>('selected-function', null)
