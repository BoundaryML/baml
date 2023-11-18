import * as vscode from 'vscode'
import net from 'net'
import * as path from 'path'
import * as fs from 'fs'
import * as os from 'os'
import { exec } from 'child_process'
import { TestRequest } from '@baml/common'

const outputChannel = vscode.window.createOutputChannel('baml-test-runner')

function __initServer(messageHandler: (data: string) => void) {
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

  private handleMessage(data: string) {
    outputChannel.appendLine(data)
  }

  public async runTest(tests: TestRequest, cwd: string) {
    const tempFilePath = path.join(os.tmpdir(), 'test_temp.py')
    fs.writeFileSync(tempFilePath, generateTestCode(tests))

    // Add filters.
    let test_filter = `-k ${tests.functions
      .flatMap((fn) => fn.tests.flatMap((test) => test.impls.map((impl) => `test_${test.name}[${fn.name}-${impl}]`)))
      .join(' or ')}`

    // Run the Python script in a child process
    let command = `python -m pytest ${tempFilePath} ${this.port_arg} ${test_filter}`
    if (fs.existsSync(path.join(cwd, 'pyproject.toml'))) {
      command = `poetry run ${command}`
    }

    this.handleMessage(`Running command: ${command}`)
    this.handleMessage(`CWD: ${cwd}`)
    this.handleMessage(`JSON: ${JSON.stringify(tests)}`)

    // Run the Python script in a child process
    // const process = spawn(pythonExecutable, [tempFilePath]);
    // Run the Python script using exec
    const cp = exec(command, {
      cwd: cwd,
    })

    cp.stdout?.on('data', (data) => {
      this.handleMessage(data)
    })
    cp.stderr?.on('data', (data) => {
      this.handleMessage(data)
    })
  }

  close() {
    if (this.server) {
      this.server.close()
    }
  }
}

function generateTestCode(test: TestRequest) {
  // For now assume we can only handle 1 function.
  console.assert(test.functions.length === 1)

  let fn = test.functions[0]

  // For now assume we can only handle 1 test.
  console.assert(fn.tests.length === 1)

  let testCase = fn.tests[0]

  // For now assume we can only handle positional params.
  console.assert(testCase.params.type === 'positional')

  return `
from baml_lib._impl.deserializer import Deserializer
from baml_client import baml
from baml_client.baml_types import I${fn.name}
from baml_client.baml_types import ${fn.input_type}

@baml.${fn.name}.test
async def test_${testCase.name}(${fn.name}Impl: I${fn.name}):
    deserializer = Deserializer[${fn.input_type}](${fn.input_type})
    param = deserializer.from_string("""${testCase.params.value}""")
    await ${fn.name}Impl(param)
    `
}

const testExecutor = new TestExecutor()

export default testExecutor
