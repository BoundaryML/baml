import { atom } from 'jotai/vanilla'
import type { WritableAtom } from 'jotai/vanilla'
import { RESET } from 'jotai/vanilla/utils'

type SetStateActionWithReset<Value> = Value | typeof RESET | ((prev: Value) => Value | typeof RESET)

const safeJSONParse = (initialValue: unknown) => (str: string) => {
  try {
    return JSON.parse(str)
  } catch (e) {
    return initialValue
  }
}

export function atomWithHash<Value>(
  key: string,
  initialValue: Value,
  options?: {
    serialize?: (val: Value) => string
    deserialize?: (str: string) => Value
    subscribe?: (callback: () => void) => () => void
    setHash?: 'default' | 'replaceState' | ((searchParams: string) => void)
  },
): WritableAtom<Value, [SetStateActionWithReset<Value>], void> {
  // Use base64 encoding for serialization
  const serialize = options?.serialize || ((val: Value) => window.btoa(encodeURIComponent(JSON.stringify(val))))
  // Use base64 decoding for deserialization
  const deserialize =
    options?.deserialize || ((str: string) => safeJSONParse(initialValue)(decodeURIComponent(window.atob(str))))

  const subscribe =
    options?.subscribe ||
    ((callback) => {
      window.addEventListener('hashchange', callback)
      return () => {
        window.removeEventListener('hashchange', callback)
      }
    })

  const setHashOption = options?.setHash
  let setHash = (searchParams: string) => {
    window.location.hash = searchParams
  }
  if (setHashOption === 'replaceState') {
    setHash = (searchParams) => {
      window.history.replaceState(
        window.history.state,
        '',
        `${window.location.pathname}${window.location.search}#${searchParams}`,
      )
    }
  }
  if (typeof setHashOption === 'function') {
    setHash = setHashOption
  }

  const isLocationAvailable = typeof window !== 'undefined' && !!window.location

  const strAtom = atom(isLocationAvailable ? new URLSearchParams(window.location.hash.slice(1)).get(key) : null)
  strAtom.onMount = (setAtom) => {
    if (!isLocationAvailable) {
      return undefined
    }
    const callback = () => {
      setAtom(new URLSearchParams(window.location.hash.slice(1)).get(key))
    }
    const unsubscribe = subscribe(callback)
    callback()
    return unsubscribe
  }

  const valueAtom = atom((get) => {
    const str = get(strAtom)
    return str === null ? initialValue : deserialize(str)
  })

  return atom(
    (get) => get(valueAtom),
    (get, set, update: SetStateActionWithReset<Value>) => {
      const nextValue =
        typeof update === 'function' ? (update as (prev: Value) => Value | typeof RESET)(get(valueAtom)) : update
      const searchParams = new URLSearchParams(window.location.hash.slice(1))
      if (nextValue === RESET) {
        set(strAtom, null)
        searchParams.delete(key)
      } else {
        const str = serialize(nextValue)
        set(strAtom, str)
        searchParams.set(key, str)
      }
      setHash(searchParams.toString())
    },
  )
}
