import * as vscode from 'vscode'
import net from 'net'
import * as path from 'path'
import * as fs from 'fs'
import * as os from 'os'
import { exec } from 'child_process'
import { TestRequest } from '@baml/common'
import { generateTestRequest } from '../plugins/language-server'

const outputChannel = vscode.window.createOutputChannel('baml-test-runner')

function __initServer(messageHandler: (data: Buffer) => void) {
  let server = net.createServer((socket) => {
    console.log('Python script connected')

    socket.on('data', messageHandler)

    socket.on('end', () => {})
  })

  server.listen(0, '127.0.0.1')

  return server
}

class TestExecutor {
  private server: net.Server | undefined

  constructor() {
    this.server = undefined
  }

  public start() {
    if (this.server !== undefined) {
      return
    }
    this.server = __initServer(this.handleMessage)
  }

  private get port_arg() {
    if (this.server !== undefined) {
      let addr = this.server.address()
      vscode.window.showInformationMessage(`Server address: ${JSON.stringify(addr)}`)
      if (typeof addr === 'string') {
        return `--pytest-baml-ipc ${addr}`
      } else if (addr) {
        return `--pytest-baml-ipc ${addr.port}`
      }
    }

    vscode.window.showErrorMessage('Server not initialized')
    return ''
  }

  private handleMessage(data: Buffer) {
    console.log(JSON.stringify(JSON.parse(data.toString()), null, 2))
  }

  public async runTest(tests: TestRequest, cwd: string) {
    const tempFilePath = path.join(os.tmpdir(), 'test_temp.py')
    const code = await generateTestRequest(tests)
    if (!code) {
      vscode.window.showErrorMessage('Could not generate test request')
      return
    }
    console.log(code)
    fs.writeFileSync(tempFilePath, code)

    // Add filters.
    let test_filter = `-k ${tests.functions
      .flatMap((fn) => fn.tests.flatMap((test) => test.impls.map((impl) => `test_${test.name}[${fn.name}-${impl}]`)))
      .join(' or ')}`

    // Run the Python script in a child process
    let command = `python -m pytest ${tempFilePath} ${this.port_arg} ${test_filter}`
    if (fs.existsSync(path.join(cwd, 'pyproject.toml'))) {
      command = `poetry run ${command}`
    }

    // Run the Python script in a child process
    // const process = spawn(pythonExecutable, [tempFilePath]);
    // Run the Python script using exec
    const cp = exec(command, {
      cwd: cwd,
    })

    cp.stdout?.on('data', (data) => {
      outputChannel.appendLine(data)
    })
    cp.stderr?.on('data', (data) => {
      outputChannel.appendLine(data)
    })
  }

  close() {
    if (this.server) {
      this.server.close()
    }
  }
}

const testExecutor = new TestExecutor()

export default testExecutor
