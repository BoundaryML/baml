package com.boundaryml.jetbrains_ext

import com.boundaryml.jetbrains_ext.cli_downloader.CliVersion
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.editor.Caret
import com.intellij.openapi.editor.EditorFactory
import com.intellij.openapi.editor.event.CaretEvent
import com.intellij.openapi.editor.event.CaretListener
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.redhat.devtools.lsp4ij.LanguageServerManager
import com.redhat.devtools.lsp4ij.LanguageServerManager.StartOptions
import com.redhat.devtools.lsp4ij.LanguageServerManager.StopOptions
import com.redhat.devtools.lsp4ij.ServerStatus
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import com.redhat.devtools.lsp4ij.installation.ServerInstallationContext
import com.redhat.devtools.lsp4ij.installation.ServerInstallationStatus
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.MediaType.Companion.toMediaType
import org.eclipse.lsp4j.jsonrpc.services.JsonNotification
import java.nio.file.Paths


// Existing data class (keep as-is)
data class PortParams(val port: Int)

// New data class for version switching
data class GeneratorVersionPayload(
    val version: String,
    val root_path: String
)

// No need for data classes - we'll build JSON directly using buildJsonObject()

class BamlLanguageClient(project: Project) :
    LanguageClientImpl(project) {

    private val log = Logger.getInstance(javaClass)
    private val languageServerService = service<BamlLanguageServerService>()
    private val httpClient = OkHttpClient()
    private val json = Json { ignoreUnknownKeys = true }

    // NB(sam): if we need to do something after language server startup, we can apply that hook here
    //    override fun handleServerStatusChanged(serverStatus: ServerStatus) {
    //        super.handleServerStatusChanged(serverStatus)
    //    }

    init {
        log.info("Initializing cursor tracking for BAML files")

        // Register global caret listener for all editors
        EditorFactory.getInstance().eventMulticaster
            .addCaretListener(object : CaretListener {
                override fun caretPositionChanged(event: CaretEvent) {
                    handleCaretPositionChanged(event)
                }
            }, project)
        log.info("Cursor tracking initialized successfully")
    }

    private fun handleCaretPositionChanged(event: CaretEvent) {
        try {
            // Get the virtual file for the editor
            val editor = event.editor
            val document = editor.document
            val virtualFile = FileDocumentManager.getInstance().getFile(document)
            
            // Only process BAML files
            if (virtualFile?.extension != "baml") {
                return
            }
            
            log.debug("Processing caret position change for BAML file: ${virtualFile.path}")
            
            // Get current port
            val port = languageServerService.port
            if (port == null) {
                log.debug("No port available yet, skipping cursor notification")
                return
            }
            
            // Get cursor position
            val caret = event.caret ?: return

            // Send POST request asynchronously
            val portValue = port // capture the non-null value
            ApplicationManager.getApplication().executeOnPooledThread {
                sendCursorNotification(portValue, virtualFile, caret)
            }
            
        } catch (e: Exception) {
            log.warn("Error handling caret position change", e)
        }
    }

    private fun sendCursorNotification(port: Int, file: VirtualFile, caret: Caret) {
        try {
            val webviewUrl = "http://localhost:$port/webview/SEND_COMMAND_TO_WEBVIEW"
            
            // See vscode-to-webview-rpc.ts for the structure of this message
            val command = buildJsonObject {
                put("source", JsonPrimitive("ide_message"))
                put("payload", buildJsonObject {
                    put("command", JsonPrimitive("update_cursor"))
                    put("content", buildJsonObject {
                        put("fileName", JsonPrimitive(file.path))
                        put("line", JsonPrimitive(caret.logicalPosition.line))
                        put("column", JsonPrimitive(caret.logicalPosition.column))
                    })
                })
            }
            
            val jsonBody = json.encodeToString(kotlinx.serialization.json.JsonObject.serializer(), command)
            
            log.debug("Sending cursor notification to $webviewUrl: $jsonBody")
            
            val requestBody = jsonBody.toRequestBody("application/json".toMediaType())
            val request = Request.Builder()
                .url(webviewUrl)
                .post(requestBody)
                .build()
            
            httpClient.newCall(request).execute().use { response ->
                if (response.isSuccessful) {
                    log.debug("Successfully sent cursor notification")
                } else {
                    log.warn("Failed to send cursor notification: HTTP ${response.code} ${response.message}")
                }
            }
        } catch (e: Exception) {
            log.debug("Error sending cursor notification", e)
        }
    }

    // Existing port notification (keep exactly as-is but use new service)
    @JsonNotification("baml/playground_port")
    fun onPort(params: PortParams) {
        log.info("Port params: ${params.port}")

        log.info("Setting port to ${params.port}")
        languageServerService.setPort(params.port)
        log.info("Set port to ${params.port}")
    }

    // Phase 2: Full version switching notification processing
    @JsonNotification("baml_src_generator_version")
    fun generatorVersionNotification(payload: GeneratorVersionPayload) {
        log.info("🔄 language server requested that we run a different version: $payload")

        // Process in background to avoid blocking LSP communication
        ApplicationManager.getApplication().executeOnPooledThread {
            processVersionSwitchRequest(payload)
        }
    }

    private fun processVersionSwitchRequest(payload: GeneratorVersionPayload) {
        if (BamlIdeConfig.shouldUseLocalLanguageServerBuild()) {
            log.info("Running in development mode, ignoring version switch request")
            return
        }

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
            // 6. Check if restart is needed (equivalent to VSCode's path comparison)
            if (languageServerService.getCurrentCliVersion() != payload.version) {
                // Update version tracking even if no restart needed
                languageServerService.updateCurrentServer(payload.version)
                // 7. Execute restart (equivalent to VSCode's executeLanguageServerRestart)
                log.info("Restarting language server with new version")
                service<BamlLanguageServerService>().setRestartingFlag(true)
                // https://github.com/redhat-developer/lsp4ij/blob/main/docs/DeveloperGuide.md#install-language-server
                // Stops the language server if it is currently starting or already started.
                //Resets the installer's internal state.
                //Executes the installation via checkInstallation(context).
                //If the server was previously running, it restarts it once the installation completes.
                val context = ServerInstallationContext()
                    .setForceInstall(true)
                LanguageServerManager.getInstance(project)
                    .install("baml-language-server", context)
            }
            log.info("Already using correct CLI version, no restart needed")

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
}
