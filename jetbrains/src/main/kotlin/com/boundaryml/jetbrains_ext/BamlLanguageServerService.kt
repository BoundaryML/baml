package com.boundaryml.jetbrains_ext

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.application.Application
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.ComponentManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.diagnostic.thisLogger
import com.intellij.openapi.editor.EditorFactory
import com.intellij.openapi.editor.event.CaretEvent
import com.intellij.openapi.editor.event.CaretListener
import com.intellij.openapi.extensions.PluginId
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.project.Project
import com.intellij.util.messages.Topic

@Service
class BamlLanguageServerService() {

    private val log = thisLogger()

    companion object {
        val PORT_TOPIC = Topic.create(
            "BAML-port",
            PortListener::class.java,
            Topic.BroadcastDirection.TO_CHILDREN
        )

        // Cache the plugin version to avoid repeated lookups
        private val BUNDLED_VERSION: String by lazy {
            val pluginId = PluginId.getId("com.boundaryml.jetbrains_ext")
            val plugin = PluginManagerCore.getPlugin(pluginId)
            thisLogger().info("Resolved bundled plugin: $plugin")
            plugin?.version ?: "0.207.0"
        }
    }



    // Existing port functionality (preserve exactly)
    @Volatile
    var port: Int? = null
        private set

    // NOTE(sam): it's important that setPort always publishes to the topic, because doing so will always
    // trigger a tool window refresh. so if a lang server reuses a previous invocation's port, we still
    // refresh the tool window.
    fun setPort(newPort: Int) {
        log.info("Setting port to: $newPort")
        port = newPort
        ApplicationManager.getApplication().messageBus
            .syncPublisher(PORT_TOPIC)
            .onPort(newPort)
    }

    @Volatile
    private var currentCliVersion: String? = null

    @Volatile
    private var isRestarting: Boolean = false

    fun getCurrentCliVersion(): String {
        return currentCliVersion ?: BUNDLED_VERSION
    }

    fun isCurrentlyRestarting(): Boolean = isRestarting

    fun updateCurrentServer(version: String) {
        log.info("Updating current server state: current=$currentCliVersion version=$version port=${this.port}")
        currentCliVersion = version
    }

    fun setRestartingFlag(restarting: Boolean) {
        log.info("Setting restart flag: $restarting")
        isRestarting = restarting
    }

    // Listener interfaces
    fun interface PortListener {
        fun onPort(port: Int)
    }


    /**
     * Handle cursor position changes in editors
     */
    private fun handleCursorChange(event: CaretEvent) {
        val editor = event.editor
        val document = editor.document
        val file = FileDocumentManager.getInstance().getFile(document)

        // Only process .baml files
        if (file?.extension != "baml") return

        val position = event.newPosition
        val fileName = file.path
        val fileText = document.text
        val line = position.line + 1  // Convert to 1-based
        val column = position.column  // Keep 0-based

        log.debug("Cursor changed in BAML file: $fileName at line $line, column $column")
    }
}