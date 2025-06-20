package com.boundaryml.jetbrains_ext

import com.intellij.openapi.diagnostic.thisLogger
import com.intellij.openapi.project.Project
import com.intellij.openapi.startup.ProjectActivity

// This runs on startup
class BamlProjectActivity : ProjectActivity {

    override suspend fun execute(project: Project) {
        thisLogger().info("BAML Jetbrains extension has started")
    }
}