package com.boundaryml.jetbrains_ext

import com.intellij.openapi.wm.ToolWindowManager
import com.redhat.devtools.lsp4ij.commands.LSPCommandAction
import org.eclipse.lsp4j.ExecuteCommandParams
import com.redhat.devtools.lsp4ij.commands.LSPCommand
import com.intellij.openapi.actionSystem.AnActionEvent

class BamlPlaygroundAction : LSPCommandAction() {

    override fun commandPerformed(command: LSPCommand, e: AnActionEvent) {
        val project = e.project ?: return
        val toolWindow = ToolWindowManager.getInstance(project)
            .getToolWindow("BAML Playground")

        val args: List<Any> = command.arguments

        toolWindow?.show {
            val ls = getLanguageServer(e)?.server ?: return@show
            ls.workspaceService.executeCommand(
                ExecuteCommandParams(command.command, command.arguments)
            )
        }
    }
}