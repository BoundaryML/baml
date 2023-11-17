import React, { useEffect, useState, useRef, useMemo } from 'react'
import { vscode } from './utils/vscode'
import {
  VSCodeButton,
  VSCodeTextArea,
  VSCodeDropdown,
  VSCodeOption,
  VSCodeDivider,
} from '@vscode/webview-ui-toolkit/react'
import { Allotment } from 'allotment'
import 'allotment/dist/style.css'

import './App.css'
import { TextArea } from '@vscode/webview-ui-toolkit'
import Playground from './Playground'
import { ParserDatabase } from './utils/parser_db'

function App() {
  const [projects, setProjects] = useState<{ root_dir: string; db: ParserDatabase }[]>([])
  const [selectedProjectId, setSelectedProjectId] = useState<string | undefined>()

  let selectedProject = useMemo(
    () =>
      selectedProjectId === undefined ? undefined : projects.find((project) => project.root_dir === selectedProjectId),
    [projects, selectedProjectId],
  )

  useEffect(() => {
    const fn = (event: any) => {
      const command = event.data.command
      const messageContent = event.data.content

      switch (command) {
        case 'setDb': {
          setProjects(messageContent.map((p: any) => ({ root_dir: p[0], db: p[1] })))
          setSelectedProjectId((prev) => (prev ?? messageContent.length > 0 ? messageContent[0][0] : undefined))
          break
        }
        case 'rmDb': {
          setProjects((prev) => prev.filter((project) => project.root_dir !== messageContent))
          break
        }
      }
    }

    window.addEventListener('message', fn)

    return () => {
      window.removeEventListener('message', fn)
    }
  }, [])

  if (!selectedProject) {
    return (
      <div>
        <h1>Projects</h1>
        <div>
          {projects.map((project) => (
            <div key={project.root_dir}>
              <button onClick={() => setSelectedProjectId(project.root_dir)}>{project.root_dir}</button>
            </div>
          ))}
          {JSON.stringify(projects, null, 2)}
        </div>
      </div>
    )
  }

  return <Playground project={selectedProject.db} />
}

export default App
