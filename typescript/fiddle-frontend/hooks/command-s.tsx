'use client'

import { useEffect } from 'react'

export function useKeybindingOverrides() {
  useEffect(() => {
    const handleKeyDown = (event: any) => {
      // Check if either Ctrl+S or Command+S is pressed
      if ((event.ctrlKey || event.metaKey) && (event.key === 's' || event.keyCode === 83)) {
        event.preventDefault()
        console.log('Custom save action triggered')
      }
      // Check if either Ctrl+W or Command+W is pressed for closing tab/window
      else if ((event.ctrlKey || event.metaKey) && event.key === 'w') {
        event.preventDefault()
        console.log('Custom close tab/window action triggered')
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [])
}
