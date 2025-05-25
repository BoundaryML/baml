package com.boundaryml.jetbrains_ext

import com.intellij.openapi.components.Service
import com.intellij.openapi.diagnostic.thisLogger
import com.intellij.openapi.project.Project

// This service runs in the background
@Service(Service.Level.PROJECT)
class BamlProjectService(project: Project) {

    init {
        thisLogger().info(MyBundle.message("projectService", project.name))
        thisLogger().info("BAML Jetbrains extension service has started")
    }

    fun getRandomNumber() = (1..100).random()
}