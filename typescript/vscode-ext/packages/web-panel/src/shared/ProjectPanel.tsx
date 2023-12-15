import { useContext, useState } from 'react'
import { ASTContext } from './ASTProvider'
import { VSCodeButton } from '@vscode/webview-ui-toolkit/react'
import { Dialog, DialogContent, DialogTrigger } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

const ProjectPanel: React.FC<{ onClick?: () => void }> = ({ onClick }) => {
  const { projects, selectedProjectId, setSelection } = useContext(ASTContext)

  return (
    <div>
      <h1>Projects</h1>
      <div>
        {projects.map((project) => (
          <div key={project.root_dir}>
            <VSCodeButton
              onClick={() => {
                setSelection(project.root_dir, undefined, undefined, undefined, undefined)
                onClick?.()
              }}
            >
              {project.root_dir}
            </VSCodeButton>
          </div>
        ))}
      </div>
    </div>
  )
}

export const ProjectToggle = () => {
  const [show, setShow] = useState<boolean>(false)

  return (
    <Dialog open={show} onOpenChange={setShow}>
      <DialogTrigger asChild={true}>
        <Button variant="outline" className="p-1 w-fit h-fit text-xs">
          Switch Projects
        </Button>
      </DialogTrigger>
      <DialogContent className="max-h-screen overflow-y-scroll bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip">
        <ProjectPanel onClick={() => setShow(false)} />
      </DialogContent>
    </Dialog>
  )
}

export default ProjectPanel
