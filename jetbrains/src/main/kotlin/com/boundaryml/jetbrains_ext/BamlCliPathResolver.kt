package com.boundaryml.jetbrains_ext

import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.project.Project
import java.nio.file.Files
import java.nio.file.Paths

object BamlCliPathResolver {
    private val logger = Logger.getInstance(javaClass)
    
    /**
     * Resolves CLI path for requested version - equivalent to VSCode's resolveCliPath
     * Simplified implementation: always returns hardcoded debug CLI path
     */
    suspend fun resolveCliPath(
        project: Project, 
        requestedVersion: String
    ): String? {
        logger.info("Resolving CLI path for version: $requestedVersion")

        if (requestedVersion == "0.205.0") {
            logger.info("resolving to 0.205.0 special logic")
            return "/Users/sam/.baml/baml-cli-0.205.0-aarch64-apple-darwin-baml-cli"
        }
        
        // Always return the hardcoded debug CLI path for simplicity
        val debugCliPath = "/Users/sam/baml/engine/target/debug/baml-cli"
        
        return if (Files.exists(Paths.get(debugCliPath))) {
            logger.info("Using hardcoded debug CLI: $debugCliPath")
            debugCliPath
        } else {
            logger.warn("Hardcoded debug CLI not found: $debugCliPath")
            null
        }
    }
}