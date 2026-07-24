import Foundation
import Testing

@testable import BamlBridge

@Test
func unhandled_spawn_error_uses_host_default() {
    var reported = false
    reportUnhandledSpawnError(payload: Data([0xFF]), cancelled: false) { _ in
        reported = true
    }
    #expect(reported)
}
