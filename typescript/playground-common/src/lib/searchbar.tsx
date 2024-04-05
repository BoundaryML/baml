import React, { useEffect, useMemo, useState } from 'react'
import { Input } from '../components/ui/input'
import { cn } from './utils'

const SearchBarWithSelector: React.FC<{
  options: {
    value: string
    label?: string
  }[]
  onChange: (updated: string) => void
}> = ({ options, onChange }) => {
  const [searchTerm, setSearchTerm] = useState('')
  const [selectedOption, setSelectedOption] = useState('')
  const [highlightedIndex, setHighlightedIndex] = useState(0)

  const filteredOptions = useMemo(
    () => options.filter((option) => (option.label ?? option.value).toLowerCase().includes(searchTerm.toLowerCase())),
    [options, searchTerm],
  )

  // Limit to displaying at most 10 options

  useEffect(() => {
    setHighlightedIndex(0) // Reset highlight when search term changes
  }, [searchTerm])

  const handleKeyDown = (event: KeyboardEvent) => {
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setHighlightedIndex((prevIndex) => Math.min(prevIndex + 1, filteredOptions.length - 1))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      setHighlightedIndex((prevIndex) => Math.max(prevIndex - 1, 0))
    } else if (event.key === 'Enter') {
      event.preventDefault()
      if (filteredOptions[highlightedIndex]) {
        setSelectedOption(filteredOptions[highlightedIndex].value)
        onChange(filteredOptions[highlightedIndex].value)
      }
    }
  }

  useEffect(() => {
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleKeyDown])

  return (
    <div className="flex flex-col gap-4 p-4 bg-primary/90 text-primary-foreground">
      <Input
        placeholder="Search..."
        value={searchTerm}
        onChange={(e) => setSearchTerm(e.target.value)}
        // Additional TailwindCSS classes can be applied if necessary
      />
      <div className="flex flex-col gap-2 overflow-x-hidden overflow-y-scroll max-h-96">
        {filteredOptions.length === 0 ? (
          <div>No options found</div>
        ) : (
          // Placeholder can be added as the first option if needed
          // Additional TailwindCSS classes can be applied if necessary
          filteredOptions.map((option, index) => (
            <div
              key={option.value}
              className={cn(
                'cursor-pointer',
                'py-0.5 px-1 rounded-md',
                highlightedIndex === index ? 'bg-secondary text-secondary-foreground' : '',
              )}
              onClick={() => {
                setSelectedOption(option.value)
                onChange(option.value)
              }}
              onMouseEnter={() => setHighlightedIndex(index)}
            >
              {option.label ?? option.value}
            </div>
          ))
        )}
      </div>
    </div>
  )
}

export default SearchBarWithSelector
