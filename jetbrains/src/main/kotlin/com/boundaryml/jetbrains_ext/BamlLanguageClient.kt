package com.boundaryml.jetbrains_ext

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import kotlinx.coroutines.runBlocking
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification
import java.nio.file.Paths

// Existing data class (keep as-is)
data class PortParams(val port: Int)

// New data class for version switching
data class GeneratorVersionPayload(
    val version: String,
    val root_path: String
)

class BamlLanguageClient(project: Project) :
    LanguageClientImpl(project) {

    private val log = Logger.getInstance(javaClass)
    private val languageServerService = project.getService(BamlLanguageServerService::class.java)

    // Existing port notification (keep exactly as-is but use new service)
    @JsonNotification("baml/port")
    fun onPort(params: PortParams) {
        log.warn("Port params: ${params.port}")

        log.warn("Setting port to ${params.port}")
        languageServerService.setPort(params.port)
        log.warn("Set port to ${params.port}")
    }

    // Phase 2: Full version switching notification processing
    @JsonNotification("baml_src_generator_version")
    fun generatorVersionNotification(payload: GeneratorVersionPayload) {
        log.warn("🔄 PROCESSING generator version notification: ${payload.version} for ${payload.root_path}")
        
        // Process in background to avoid blocking LSP communication
        ApplicationManager.getApplication().executeOnPooledThread {
            processVersionSwitchRequest(payload)
        }
    }
    
    private fun processVersionSwitchRequest(payload: GeneratorVersionPayload) {
        try {
            log.warn("Processing version switch request: ${payload.version}")
            
            // 0. Skip version switching in debug mode - preserve existing debug behavior
//            if (BamlIdeConfig.isDebugMode) {
//                log.warn("Debug mode detected - skipping version switching to preserve existing debug logic")
//                return
//            }
            
            // 1. Validate notification is for current project (equivalent to VSCode's isPathWithinParent)
            if (!isNotificationForCurrentProject(payload.root_path)) {
                log.debug("Ignoring version notification for different project: ${payload.root_path}")
                return
            }
            
            // 2. Check if restart already in progress (equivalent to VSCode's isRestarting flag)
            if (languageServerService.isCurrentlyRestarting()) {
                log.info("Language server restart already in progress, ignoring request")
                return
            }
            
            // 3. Validate semantic version (equivalent to VSCode's semver.valid check)
            if (!isValidSemanticVersion(payload.version)) {
                log.warn("Invalid semantic version received: ${payload.version}")
                return
            }
            
            // 4. Check minimum version requirement (equivalent to VSCode's >= 0.86.0 check)  
            if (!isMinimumVersionSupported(payload.version)) {
                log.warn("Ignoring version ${payload.version} - below minimum supported version")
                return
            }
            
            // 5. Resolve target CLI path (equivalent to VSCode's resolveCliPath call)
            runBlocking {
                val targetCliPath = BamlCliPathResolver.resolveCliPath(project, payload.version)
                
                if (targetCliPath == null) {
                    log.warn("No suitable CLI found for version: ${payload.version}")
                    return@runBlocking
                }
                
                // 6. Check if restart is needed (equivalent to VSCode's path comparison)
                if (!languageServerService.shouldRestartForVersion(targetCliPath)) {
                    log.warn("Already using correct CLI version, no restart needed")
                    // Update version tracking even if no restart needed
                    languageServerService.updateCurrentServer(targetCliPath, payload.version)
                    return@runBlocking
                }
                
                // 7. Execute restart (equivalent to VSCode's executeLanguageServerRestart)
                executeLanguageServerRestart(payload.version, targetCliPath)
            }
            
        } catch (e: Exception) {
            log.error("Error processing version switch request", e)
        }
    }
    
    private fun isNotificationForCurrentProject(rootPath: String): Boolean {
        val projectBasePath = project.basePath ?: return false
        return try {
            val notificationPath = Paths.get(rootPath).normalize()
            val projectPath = Paths.get(projectBasePath).normalize()
            // Check if paths overlap (either direction)
            notificationPath.startsWith(projectPath) || projectPath.startsWith(notificationPath)
        } catch (e: Exception) {
            log.warn("Error validating project path: $rootPath", e)
            false
        }
    }
    
    private fun isValidSemanticVersion(version: String): Boolean {
        // Basic semantic version validation (x.y.z pattern)
        return version.matches(Regex("\\d+\\.\\d+\\.\\d+.*"))
    }
    
    private fun isMinimumVersionSupported(version: String): Boolean {
        // Only versions 0.86.0+ support this notification (like VSCode)
        return try {
            val versionParts = version.split(".")
            if (versionParts.size < 3) return false
            val major = versionParts[0].toInt()
            val minor = versionParts[1].toInt()
            major > 0 || (major == 0 && minor >= 86)
        } catch (e: Exception) {
            false
        }
    }
    
    private fun executeLanguageServerRestart(version: String, targetCliPath: String) {
        log.warn("Executing language server restart: version=$version, path=$targetCliPath")
        
        // Execute restart in background to avoid blocking LSP communication
        ApplicationManager.getApplication().executeOnPooledThread {
            performRestart(version, targetCliPath)
        }
    }
    
    private fun performRestart(version: String, targetCliPath: String) {
        languageServerService.setRestartingFlag(true)
        
        try {
            log.warn("Starting language server restart for version: $version")
            
            // 1. Update server state to track the new CLI path and version
            languageServerService.updateCurrentServer(targetCliPath, version)
            
            // 2. Use LSP4IJ to restart the server
            restartLanguageServer()
            
            log.warn("Successfully restarted language server with version: $version")
            showSuccessNotification(version)
            
        } catch (e: Exception) {
            log.error("Failed to restart language server", e)
            showErrorNotification(version, e.message ?: "Unknown error")
        } finally {
            languageServerService.setRestartingFlag(false)
        }
    }
    
    private fun restartLanguageServer() {
        // For Phase 3 implementation: Since BamlLanguageServer already checks the service 
        // for dynamic CLI path in its init block, we just need to trigger a restart.
        // For now, we'll implement this as a placeholder and focus on the state management.
        
        log.warn("Language server restart triggered - implementation pending")
        
        // The actual restart implementation would:
        // 1. Use the LSP4IJ framework to stop the current server
        // 2. Start a new server instance, which will automatically pick up the new CLI path
        //    from languageServerService.getCurrentExecutingCliPath() in BamlLanguageServer.init
        
        // For now, just log that restart would happen
        throw RuntimeException("Language server restart not yet implemented - this is expected for Phase 2")
    }
    
    private fun showSuccessNotification(version: String) {
        try {
            NotificationGroupManager.getInstance()
                .getNotificationGroup("BAML Version Switch")
                .createNotification("Switched to BAML CLI version $version", NotificationType.INFORMATION)
                .notify(project)
        } catch (e: Exception) {
            log.warn("Failed to show success notification", e)
        }
    }
    
    private fun showErrorNotification(version: String, error: String) {
        try {
            NotificationGroupManager.getInstance()
                .getNotificationGroup("BAML Version Switch")  
                .createNotification("Failed to switch to BAML CLI version $version: $error", NotificationType.ERROR)
                .notify(project)
        } catch (e: Exception) {
            log.warn("Failed to show error notification", e)
        }
    }
}
