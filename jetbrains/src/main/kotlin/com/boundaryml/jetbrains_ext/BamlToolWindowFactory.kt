package com.boundaryml.jetbrains_ext

import com.intellij.openapi.diagnostic.thisLogger
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory
import com.intellij.ui.jcef.JBCefBrowser
import java.awt.BorderLayout
import javax.swing.JPanel

private const val PLACEHOLDER_HTML = """
    <!DOCTYPE html>
    <html lang="en">
      <head>
        <meta charset="UTF-8" />
        <title>Loading BAML Playground…</title>
        <style>
          body {
            margin: 0;
            height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            flex-direction: column;
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: #1e1e20;
            color: #d0d0d0;
          }
          .spinner {
            width: 48px;
            height: 48px;
            border: 6px solid rgba(255,255,255,.2);
            border-top-color: #7b61ff;
            border-radius: 50%;
            animation: spin 1s linear infinite;
            margin-bottom: 24px;
          }
          @keyframes spin { to { transform: rotate(360deg); } }
          h1 { font-size: 1.2rem; letter-spacing: .02em; margin: 0; }
        </style>
      </head>
      <body>
        <div class="spinner"></div>
        <h1>Starting BAML Playground…</h1>
      </body>
    </html>
"""

class BamlToolWindowFactory : ToolWindowFactory {

    init {
        thisLogger().warn("Don't forget to remove all non-needed sample code files with their corresponding registration entries in `plugin.xml`.")
    }

    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val browser = JBCefBrowser().apply {
            loadHTML(PLACEHOLDER_HTML.trimIndent())
        }

        // show browser in a tool window
        val panel = JPanel(BorderLayout()).apply { add(browser.component, BorderLayout.CENTER) }
        val content = ContentFactory.getInstance().createContent(panel, null, false)
        toolWindow.contentManager.addContent(content)

        val savedPort = project.getService(BamlGetPortService::class.java).port
        if (savedPort != null) {
            // LS was up before the tool-window opened
            browser.loadURL("http://localhost:$savedPort/")
        } else {
            // LS not ready yet wait for a port message
            val busConnection = project.messageBus.connect(toolWindow.disposable)
            busConnection.subscribe(
                BamlGetPortService.TOPIC,
                BamlGetPortService.Listener { port ->
                    browser.loadURL("http://localhost:$port/")
                    busConnection.disconnect()        // one-shot, avoid duplicates
                }
            )
        }

        Disposer.register(toolWindow.disposable, browser)
    }

    override fun shouldBeAvailable(project: Project) = true

    class BamlToolWindow(toolWindow: ToolWindow) {

        private val browser = JBCefBrowser()

        init {
            browser.loadHTML(
                PLACEHOLDER_HTML.trimIndent()
            )

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