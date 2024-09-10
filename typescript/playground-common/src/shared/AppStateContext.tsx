import React, { createContext, useState, useContext } from 'react'

interface AppStateContextProps {
  showTokens: boolean
  setShowTokens: React.Dispatch<React.SetStateAction<boolean>>
  showWhitespace: boolean
  // setShowWhitespace: React.Dispatch<React.SetStateAction<boolean>>
  showCurlRequest: boolean
  setShowCurl: React.Dispatch<React.SetStateAction<boolean>>
  showTestResults: boolean
  setShowTestResults: React.Dispatch<React.SetStateAction<boolean>>
}

const AppStateContext = createContext<AppStateContextProps | undefined>(undefined)

export const AppStateProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [showTokens, setShowTokens] = useState(false)
  const showWhitespace = false
  // const [showWhitespace, setShowWhitespace] = useState(false)
  const [showCurlRequest, setShowCurl] = useState(false)
  const [showTestResults, setShowTestResults] = useState(true)

  return (
    <AppStateContext.Provider
      value={{
        showTokens,
        setShowTokens,
        showWhitespace,
        showCurlRequest,
        setShowCurl,
        showTestResults,
        setShowTestResults,
      }}
    >
      {children}
    </AppStateContext.Provider>
  )
}

export const useAppState = () => {
  const context = useContext(AppStateContext)
  if (!context) {
    throw new Error('useAppState must be used within an AppStateProvider')
  }
  return context
}
