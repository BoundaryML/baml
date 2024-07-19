import { exec } from 'child_process'
import type { URI } from 'vscode-uri'

export function cliBuild(
  cliPath: string,
  workspacePath: URI | null,
  onError?: (errorMessage: string) => void,
  onSuccess?: () => void,
) {
  const buildCommand = `${cliPath} generate --from ${workspacePath?.fsPath}`

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
        console.error(`Error running baml cli script: ${JSON.stringify(error, null, 2)}`)
        onError?.(`baml-cli error: ${error.message}`)
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
  const buildCommand = `${cliPath} --version`

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
  const buildCommand = `${cliPath} version --check --output json`

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
