package com.boundaryml.jetbrains_ext

import com.intellij.openapi.components.Service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import com.intellij.util.messages.Topic

@Service(Service.Level.PROJECT)
class BamlLanguageServerService(private val project: Project) {
    
    private val logger = Logger.getInstance(javaClass)

    companion object {
        val PORT_TOPIC = Topic.create(
            "BAML-port",
            PortListener::class.java,
            Topic.BroadcastDirection.NONE
        )
        
        val VERSION_TOPIC = Topic.create(
            "BAML-version",
            VersionListener::class.java,
            Topic.BroadcastDirection.NONE
        )
    }

    // Existing port functionality (preserve exactly)
    @Volatile
    var port: Int? = null
        private set

    fun setPort(newPort: Int) {
        logger.warn("Setting port to: $newPort")
        port = newPort
        project.messageBus
            .syncPublisher(PORT_TOPIC)
            .onPort(newPort)
    }

    // New version switching state
    @Volatile
    private var currentExecutingCliPath: String? = null
    @Volatile
    private var currentCliVersion: String? = null
    @Volatile
    private var isRestarting: Boolean = false

    fun getCurrentExecutingCliPath(): String? = currentExecutingCliPath
    fun getCurrentCliVersion(): String? = currentCliVersion
    fun isCurrentlyRestarting(): Boolean = isRestarting

    fun updateCurrentServer(cliPath: String, version: String) {
        logger.warn("Updating current server state: version=$version, path=$cliPath")
        currentExecutingCliPath = cliPath
        currentCliVersion = version
        project.messageBus
            .syncPublisher(VERSION_TOPIC)
            .onVersionChanged(version, cliPath)
    }

    fun setRestartingFlag(restarting: Boolean) {
        logger.warn("Setting restart flag: $restarting")
        isRestarting = restarting
    }

    fun shouldRestartForVersion(targetCliPath: String): Boolean {
        val shouldRestart = targetCliPath != currentExecutingCliPath
        logger.warn("Version switch decision: shouldRestart=$shouldRestart (current=$currentExecutingCliPath, target=$targetCliPath)")
        return shouldRestart
    }

    // Listener interfaces
    fun interface PortListener { 
        fun onPort(port: Int) 
    }
    
    fun interface VersionListener { 
        fun onVersionChanged(version: String, cliPath: String) 
    }
}