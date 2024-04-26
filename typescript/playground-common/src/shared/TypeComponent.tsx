import { useContext } from 'react'
import { ASTContext } from './ASTProvider'
import Link from './Link'

const TypeComponent: React.FC<{ typeString: string }> = ({ typeString }) => {
  const {
    db: { classes, enums },
  } = useContext(ASTContext)

  const elements: React.ReactNode[] = []
  const regex = /(\w+)/g
  let lastIndex = 0

  typeString.replace(regex, (match, className, index) => {
    // Add text before the match as plain string
    if (index > lastIndex) {
      elements.push(typeString.substring(lastIndex, index))
    }

    // Check if the class name matches any in the classes array
    const matchedClass = classes.find((cls) => cls.name.value === className)
    const matchedEnum = enums.find((enm) => enm.name.value === className)
    if (matchedClass) {
      elements.push(Link({ item: matchedClass.name, className: 'whitespace-nowrap' }))
    } else if (matchedEnum) {
      elements.push(Link({ item: matchedEnum.name, className: 'whitespace-nowrap' }))
    } else {
      elements.push(className)
    }

    lastIndex = index + match.length
    return match
  })

  // Add any remaining text
  if (lastIndex < typeString.length) {
    elements.push(typeString.substring(lastIndex))
  }

  return <>{elements}</>
}

export default TypeComponent
