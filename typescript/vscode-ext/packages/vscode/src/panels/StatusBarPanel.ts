import * as vscode from 'vscode'

const packageJson = require('../../../package.json') // eslint-disable-line

class StatusBarPanel {
  public static readonly instance = new StatusBarPanel()
  private _statusBarItem: vscode.StatusBarItem

  constructor() {
    this._statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100)
    this._statusBarItem.command = 'baml.openBamlPanel'
    this._statusBarItem.text = 'BAML'
    this._statusBarItem.hide()
  }

  public get panel() {
    return this._statusBarItem
  }

  public setStatus(
    value:
      | 'pass'
      | {
          status: 'fail' | 'warn'
          count: number
        },
  ) {
    if (value === 'pass') {
      this._statusBarItem.text = `$(check) BAML ${packageJson.version}`
      this._statusBarItem.backgroundColor = undefined
    } else if (value.status === 'fail') {
      this._statusBarItem.text = `$(error)${value.count} BAML ${packageJson.version}`
      this._statusBarItem.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground')
    } else if (value.status === 'warn') {
      this._statusBarItem.text = `$(warning)${value.count} BAML ${packageJson.version}`
      this._statusBarItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground')
    }
    this._statusBarItem.show()
  }

  dispose() {
    this._statusBarItem.dispose()
  }
}

export default StatusBarPanel
