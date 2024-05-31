import os from 'os'
import { PostHog } from 'posthog-node'
import { type Disposable, workspace } from 'vscode'
import * as vscode from 'vscode'
import { getProjectHash } from './hashes'
type TelemetryLevel = 'off' | 'crash' | 'error' | 'all' | undefined

const client = new PostHog('phc_732PWG6HFZ75S7h0TK2AuqRVkqZDiD4WePE9gXYJkOu')

export interface TelemetryEvent {
  event: string
  properties: any
}

export default class TelemetryReporter {
  private userOptIn = false
  private readonly configListener: Disposable

  private static TELEMETRY_SECTION_ID = 'telemetry'
  private static TELEMETRY_SETTING_ID = 'telemetryLevel'
  // Deprecated since https://code.visualstudio.com/updates/v1_61#_telemetry-settings
  private static TELEMETRY_OLD_SETTING_ID = 'enableTelemetry'
  private telemetryProps: any = {}

  constructor(
    private extensionId: string,
    private extensionVersion: string,
  ) {
    this.updateUserOptIn()
    this.configListener = workspace.onDidChangeConfiguration(() => this.updateUserOptIn())
  }

  public async initialize(): Promise<void> {
    if (this.userOptIn) {
      const machine_id = vscode.env.machineId
      const properties = {
        extension: this.extensionId,
        version: this.extensionVersion,
        project_hash: await getProjectHash(),
        machine_id: machine_id,
        session_id: vscode.env.sessionId,
        vscode_version: vscode.version,
        os_info: {
          release: os.release(),
          platform: os.platform(),
          arch: os.arch(),
        },
      }
      this.telemetryProps = properties
      client.capture({
        event: 'extension_loaded',
        distinctId: machine_id,
        properties: properties,
      })
      client.flush()
    }
  }

  public async sendTelemetryEvent(data: TelemetryEvent): Promise<void> {
    if (this.userOptIn) {
      client.capture({
        event: data.event,
        distinctId: vscode.env.machineId,
        properties: {
          ...this.telemetryProps,
          ...data.properties,
        },
      })
      client.flush()
    }
  }

  private updateUserOptIn() {
    const telemetrySettings = workspace.getConfiguration(TelemetryReporter.TELEMETRY_SECTION_ID)
    const isTelemetryEnabled = telemetrySettings.get<boolean>(TelemetryReporter.TELEMETRY_OLD_SETTING_ID)
    // Only available since https://code.visualstudio.com/updates/v1_61#_telemetry-settings
    const telemetryLevel = telemetrySettings.get<string>(TelemetryReporter.TELEMETRY_SETTING_ID) as TelemetryLevel

    // `enableTelemetry` is either true or false (default = true). Deprecated since https://code.visualstudio.com/updates/v1_61#_telemetry-settings
    // It is replaced by `telemetryLevel`, only available since v1.61 (default = 'all')
    // https://code.visualstudio.com/docs/getstarted/telemetry
    // To enable Telemetry:
    // We check that
    // `enableTelemetry` is true and `telemetryLevel` falsy -> enabled
    // `enableTelemetry` is true and `telemetryLevel` set to 'all' -> enabled
    // anything else falls back to disabled.
    const isTelemetryEnabledWithOldSetting = isTelemetryEnabled && !telemetryLevel
    const isTelemetryEnabledWithNewSetting = isTelemetryEnabled && telemetryLevel && telemetryLevel === 'all'
    if (isTelemetryEnabledWithOldSetting || isTelemetryEnabledWithNewSetting) {
      this.userOptIn = true
      console.info('Telemetry is enabled for BAML extension')
    } else {
      this.userOptIn = false
      console.info('Telemetry is disabled for BAML extension')
    }
  }

  public async dispose(): Promise<void> {
    this.configListener.dispose()
    await client.shutdownAsync()
  }
}
