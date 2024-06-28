import React, { createContext, useState, useContext } from 'react'

interface AppStateContextProps {
  showTokens: boolean
  setShowTokens: React.Dispatch<React.SetStateAction<boolean>>
  showWhitespace: boolean
  setShowWhitespace: React.Dispatch<React.SetStateAction<boolean>>
  showCurlRequest: boolean
  setShowCurl: React.Dispatch<React.SetStateAction<boolean>>
}

const AppStateContext = createContext<AppStateContextProps | undefined>(undefined)

export const AppStateProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [showTokens, setShowTokens] = useState(false)
  const [showWhitespace, setShowWhitespace] = useState(false)
  const [showCurlRequest, setShowCurl] = useState(false)

  return (
    <AppStateContext.Provider
      value={{
        showTokens,
        setShowTokens,
        showWhitespace,
        setShowWhitespace,
        showCurlRequest,
        setShowCurl,
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
