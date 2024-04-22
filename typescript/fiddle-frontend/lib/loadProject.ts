"use server"
import { loadUrl } from "@/app/actions"
import { BAMLProject, exampleProjects } from "./exampleProjects"
export async function loadProject(params: { project_id: string }, chooseDefault: boolean = false) {
  let data: BAMLProject = exampleProjects[0]
  const id = params.project_id
  if (id) {
    const exampleProject = exampleProjects.find((p) => p.id === id)
    if (exampleProject) {
      data = exampleProject
    } else {
      data = await loadUrl(id)
    }
  } else {
    if (chooseDefault) {
      data = exampleProjects[0]
    }
  }
  return data
}
