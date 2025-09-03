package com.boundaryml.jetbrains_ext

object BamlIdeConfig {
    val isDebugMode: Boolean
    
    init {
        val debugModeEnv = System.getenv("VSCODE_DEBUG_MODE")
        isDebugMode = debugModeEnv == "true"
        println("BamlIdeConfig: VSCODE_DEBUG_MODE=${debugModeEnv ?: "(unset)"}, isDebugMode=$isDebugMode")
    }
    
    fun getPlaygroundUrl(port: Int): String {
        return "http://localhost:$port/"
    }
}