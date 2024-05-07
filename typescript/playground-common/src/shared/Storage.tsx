import { atom, useAtom } from 'jotai'
import { atomWithStorage, createJSONStorage } from 'jotai/utils'

export const availableProjectsStorageAtom = atomWithStorage<string[]>('available-projects', [])

export const selectedProjectStorageAtom = atomWithStorage<string | null>('selected-project', null)

export const selectedFunctionStorageAtom = atomWithStorage<string | null>('selected-function', null)

const secretStorage = createJSONStorage<Record<string, string>>(() => sessionStorage)

export const envvarStorageAtom = atomWithStorage<Record<string, string>>('environment-variables', {}, secretStorage)
