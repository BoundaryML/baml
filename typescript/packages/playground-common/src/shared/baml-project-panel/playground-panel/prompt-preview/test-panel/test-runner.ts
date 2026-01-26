/**
 * Test Runner - Simplified SDK Integration
 *
 * This file provides a React hook for running BAML tests.
 * All test execution logic and state management has been moved to the SDK.
 *
 * Responsibilities:
 * - Bridge between React components and SDK
 * - Handle WASM panic detection and auto-cancel (React-specific)
 * - Pass API keys from atoms to SDK
 */

import { useAtomValue } from 'jotai'
import { useCallback, useEffect } from 'react'
import { useBAMLSDK } from '../../../../../sdk/hooks'
import { apiKeysAtom } from '../../../../../components/api-keys-dialog/atoms'
import { wasmPanicAtom } from '../../../atoms'
import { vscode } from '../../../vscode'
import { isParallelTestsEnabledAtom } from './atoms'

export const useRunBamlTests = () => {
  const sdk = useBAMLSDK()
  const apiKeys = useAtomValue(apiKeysAtom)
  const panicState = useAtomValue(wasmPanicAtom)
  const isParallelTestsEnabled = useAtomValue(isParallelTestsEnabledAtom)

  // Automatically cancel tests when WASM panics
  useEffect(() => {
    if (panicState) {
      console.error('[WASM Panic] Detected panic during test run, cancelling tests:', panicState.msg)

      // Send telemetry about the panic
      vscode.sendTelemetry({
        action: 'wasm_panic',
        data: {
          panic_message: panicState.msg,
          timestamp: panicState.timestamp,
          during_test_execution: true,
        },
      })

      // Cancel tests via SDK
      sdk.tests.cancel()
    }
  }, [panicState, sdk])

  /**
   * Run tests via SDK
   * SDK automatically handles:
   * - Test history creation and updates
   * - Running state management
   * - Watch notifications and enrichment
   * - Highlighted blocks
   * - Flash ranges and VSCode integration
   * - Partial response streaming
   * - Error handling and cancellation
   */
  const runTests = useCallback(
    async (tests: Array<{ functionName: string; testName: string }>) => {
      console.log('[useRunBamlTests] Running tests with apiKeys:', {
        hasBoundaryProxyUrl: 'BOUNDARY_PROXY_URL' in apiKeys,
        boundaryProxyUrl: apiKeys.BOUNDARY_PROXY_URL,
        keyCount: Object.keys(apiKeys).length,
      })
      await sdk.tests.runAll(tests, {
        apiKeys,
        parallel: isParallelTestsEnabled,
      })
    },
    [sdk, apiKeys, isParallelTestsEnabled]
  )

  /**
   * Cancel currently running tests
   */
  const cancelTests = useCallback(() => {
    sdk.tests.cancel()
  }, [sdk])

  return { runTests, cancelTests }
}
