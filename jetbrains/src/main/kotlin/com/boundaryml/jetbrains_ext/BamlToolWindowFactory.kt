package com.boundaryml.jetbrains_ext

import com.intellij.openapi.diagnostic.thisLogger
import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory
import com.intellij.ui.jcef.JBCefBrowser
import java.awt.BorderLayout
import java.awt.Container
import javax.swing.JPanel

class BamlToolWindowFactory : ToolWindowFactory {

    init {
        thisLogger().warn("Don't forget to remove all non-needed sample code files with their corresponding registration entries in `plugin.xml`.")
    }

    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val myToolWindow = BamlToolWindow(toolWindow)
        val content = ContentFactory.getInstance().createContent(myToolWindow.getContent(), null, false)
        toolWindow.contentManager.addContent(content)
    }

    override fun shouldBeAvailable(project: Project) = true

    class BamlToolWindow(toolWindow: ToolWindow) {

        private val browser = JBCefBrowser()

        init {
            var htmlContent = """
                          <!DOCTYPE html>
                          <html lang="en">
                            <head>
                              <meta charset="UTF-8" />
                              <meta name="viewport" content="width=device-width, initial-scale=1.0" />
                              <title>Hello World</title>
                            </head>
                            <body>
                              <div id="root">TODO: render the BAML playground here and wire up the vscode provider bridge</div>
                            </body>
                          </html>
            """.trimIndent()
            browser.loadHTML(htmlContent)
        }

        fun getContent(): JPanel {
            return JPanel(BorderLayout()).apply {
                add(browser.component, BorderLayout.CENTER)
            }
        }


        // This approach doesn't work.
        // We need to follow instructions here and implement resource loaders
        // https://plugins.jetbrains.com/docs/intellij/embedded-browser-jcef.html#loading-resources-from-plugin-distribution
        private fun loadHtmlFromResources(): String {
            // Load the HTML file from the `resources/web-panel/index.html`
            val stylesUri = javaClass.getResource("/web-panel/index.css")!!.toURI()
            val scriptUri = javaClass.getResource("/web-panel/index.js")!!.toURI()

            val htmlContent = """
                          <!DOCTYPE html>
                          <html lang="en">
                            <head>
                              <meta charset="UTF-8" />
                              <meta name="viewport" content="width=device-width, initial-scale=1.0" />
                              <title>Hello World</title>
                            </head>
                            <body>
                              <div id="root">Waiting for react (unimplemented)</div>
                            </body>
                          </html>
            """.trimIndent()

            return htmlContent;
        }
    }
}