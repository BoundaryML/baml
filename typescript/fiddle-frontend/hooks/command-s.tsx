'use client'

import { useEffect } from 'react'

export function useCommandS() {
  useEffect(() => {
    const handleKeyDown = (event: any) => {
      // Check if either Ctrl+S or Command+S is pressed
      if ((event.ctrlKey || event.metaKey) && (event.key === 's' || event.keyCode === 83)) {
        event.preventDefault()
        console.log('Custom save action triggered')
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => {
      window.removeEventListener('keydown', handleKeyDown)
    }
  }, [])
}
