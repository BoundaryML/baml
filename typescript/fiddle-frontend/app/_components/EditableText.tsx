import { cn } from '@/lib/utils'
import { useEffect, useState } from 'react'

type EditableProps = {
  text: string
  type: string
  placeholder: string
  children: React.ReactNode
  childRef: React.RefObject<HTMLDivElement>
  className?: string
}

export const Editable = ({ text, type, placeholder, children, childRef, className }: EditableProps) => {
  const [isEditing, setEditing] = useState(false)

  useEffect(() => {
    if (childRef && childRef.current && isEditing === true) {
      childRef.current.focus()
    }
  }, [isEditing, childRef])

  const handleKeyDown = (event: React.KeyboardEvent, type: string) => {
    const { key } = event
    const keys = ['Escape', 'Tab']
    const enterKey = 'Enter'
    const allKeys = [...keys, enterKey]
    if ((type === 'textarea' && keys.indexOf(key) > -1) || (type !== 'textarea' && allKeys.indexOf(key) > -1)) {
      setEditing(false)
    }
  }

  return (
    <section>
      {isEditing ? (
        <div onBlur={() => setEditing(false)} onKeyDown={(e) => handleKeyDown(e, type)}>
          {children}
        </div>
      ) : (
        <div
          className={cn(
            `pt-1 pl-4 text-lg hover:text-muted-foreground font-semibold text-foreground editable-${type}`,
            className,
          )}
          onClick={() => setEditing(true)}
        >
          <span className={`${text ? 'text-foreground' : 'text-gray-500'}`}>
            {text || placeholder || 'Editable content'}
          </span>
        </div>
      )}
    </section>
  )
}
