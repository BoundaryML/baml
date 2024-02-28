import { exec } from 'child_process'
import { URI } from 'vscode-uri'

export function cliBuild(
  cliPath: string,
  workspacePath: URI | null,
  onError?: (errorMessage: string) => void,
  onSuccess?: () => void,
) {
  let buildCommand = `${cliPath} build`

  if (!workspacePath) {
    return
  }

  exec(
    buildCommand,
    {
      cwd: workspacePath.fsPath,
    },
    (error: Error | null, stdout: string, stderr: string) => {
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
    },
  )
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

export function cliCheckForUpdates(
  cliPath: string,
  onError?: (errorMessage: string) => void,
  onSuccess?: (ver: string) => void,
) {
  let buildCommand = `${cliPath} version --check --output json`

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
