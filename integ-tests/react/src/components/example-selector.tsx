'use client'

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { useAtom } from 'jotai'
import { exampleParser, getExampleDisplayName, hookConfigMap } from '@/lib/store'
import { useCallback } from 'react'
import { FunctionNames } from '../../baml_client/react/hooks'
import { useQueryState } from 'nuqs'

export function ExampleSelector() {
  const [selectedExample, setSelectedExample] = useQueryState('example', exampleParser)

  const handleExampleChange = useCallback((value: string) => {
    setSelectedExample(value as FunctionNames)
  }, [setSelectedExample])

  return (
    <div className='text-center space-y-4'>
      <h1 className='text-4xl font-bold tracking-tight'>BAML + Next.js Integration</h1>
      <p className='text-lg text-muted-foreground'>Select an example below to get started.</p>
      <div className='w-[200px] mx-auto'>
        <Select value={selectedExample} onValueChange={handleExampleChange}>
          <SelectTrigger>
            <SelectValue placeholder='Select an example' />
          </SelectTrigger>
          <SelectContent>
            {Object.keys(hookConfigMap).map((exampleKey) => (
              <SelectItem key={exampleKey} value={exampleKey}>
                {getExampleDisplayName(exampleKey as FunctionNames)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </div>
  )
}