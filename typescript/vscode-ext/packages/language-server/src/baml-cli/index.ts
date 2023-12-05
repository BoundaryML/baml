import { exec } from 'child_process'

export function cliBuild(
  cliPath: string,
  workspacePath: string | null,
  onError?: (errorMessage: string) => void,
  onSuccess?: () => void,
) {
  let buildCommand = `${cliPath} build`

  if (!workspacePath) {
    return
  }
  let options = {
    cwd: workspacePath,
  }

  exec(buildCommand, options, (error: Error | null, stdout: string, stderr: string) => {
    if (stdout) {
      console.log(stdout)
      // outputChannel.appendLine(stdout);
    }

    if (stderr) {
      // our CLI is by default logging everything to stderr
      console.info(stderr)
    }

    if (error) {
      console.error(`Error running the build script: ${JSON.stringify(error, null, 2)}`)
      onError?.(`Baml build error`)
      return
    } else {
      if (onSuccess) {
        onSuccess()
      }
    }
  })
}

export function cliVersion(
  cliPath: string,
  onError?: (errorMessage: string) => void,
  onSuccess?: (ver: string) => void,
) {
  let buildCommand = `${cliPath} --version`

  exec(buildCommand, (error: Error | null, stdout: string, stderr: string) => {
    if (stderr) {
      // our CLI is by default logging everything to stderr
      console.info(stderr)
    }

    if (error) {
      onError?.(`Baml cli error`)
      return
    } else {
      if (onSuccess) {
        onSuccess(stdout)
      }
    }
  })
}
