package com.boundaryml.jetbrains_ext

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.thisLogger
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.content.ContentFactory
import com.intellij.ui.jcef.JBCefBrowser
import java.awt.BorderLayout
import java.awt.FlowLayout
import javax.swing.JButton
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
        <p>You may need to open a BAML file if this does not load.</p>
      </body>
    </html>
"""

private const val VITE_HOT_RELOAD_HTML = """
<!DOCTYPE html>
<html lang="en">

<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>BAML Playground</title>
  <script type="module">
    import RefreshRuntime from "http://localhost:3030/@react-refresh"
    RefreshRuntime.injectIntoGlobalHook(window)
    ${"window.\$RefreshReg\$ = () => {}"}
    ${"window.\$RefreshSig\$ = () => (type) => type"}
    window.__vite_plugin_react_preamble_installed__ = true
  </script>
  <script type="module" crossorigin src="http://localhost:3030/src/main.tsx"></script>
  <link rel="stylesheet" crossorigin href="http://localhost:3030/src/main.tsx">
</head>

<body>
  <div id="root"></div>
</body>

</html>
"""

class BamlToolWindowFactory : ToolWindowFactory {
    private val log = thisLogger()

    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val browser = JBCefBrowser().apply {
            val savedPort = service<BamlLanguageServerService>().port
            if (savedPort != null) {
                loadURL(BamlIdeConfig.getPlaygroundUrl(savedPort))
            } else {
                loadHTML(PLACEHOLDER_HTML.trimIndent())
            }
        }

        // Create control panel with conditional debug buttons
        val controlPanel = JPanel(FlowLayout(FlowLayout.RIGHT))

        if (BamlIdeConfig.shouldShowToolWindowDebuggers()) {
            // Create reload button
            val reloadButton = JButton("Reload").apply {
                addActionListener {
                    val currentTime = java.time.LocalDateTime.now()
                    val savedPort = service<BamlLanguageServerService>().port
                    log.debug("playground reload at ${currentTime}, port is $savedPort")
                    if (savedPort != null) {
                        browser.loadURL(BamlIdeConfig.getPlaygroundUrl(savedPort))
                    } else {
                        browser.loadHTML("<p>Port not ready</p>")
                    }
                    log.debug("playground reload done")
                }
            }

            // vite hot reload
            val viteButton = JButton("Vite").apply {
                addActionListener {
                    browser.loadHTML(VITE_HOT_RELOAD_HTML.trimIndent())
                }
            }


            // Create lorem ipsum button
            val loremButton = JButton("Lorem Ipsum").apply {
                addActionListener {
                    val currentTime = java.time.LocalDateTime.now()
                    log.debug("lorem button clicked at $currentTime")
                    browser.loadHTML(
                        """
                        <!DOCTYPE html>
                        <html>
                        <head><title>Lorem Ipsum</title></head>
                        <body style="font-family: Arial, sans-serif; padding: 20px; color: white;">
                            <h1>Lorem Ipsum</h1>
                            <p><strong>Generated at:</strong> $currentTime</p>
                            <p>Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.</p>
                        </body>
                        </html>
                    """.trimIndent()
                    )
                }
            }

            val openDevToolsButton = JButton("Open DevTools").apply {
                addActionListener {
                    browser.openDevtools()
                }
            }

            controlPanel.add(reloadButton)
            controlPanel.add(viteButton)
            controlPanel.add(loremButton)
            controlPanel.add(openDevToolsButton)
        }

        // Create main panel with controls and browser
        val mainPanel = JPanel(BorderLayout()).apply {
            if (BamlIdeConfig.shouldShowToolWindowDebuggers()) {
                add(controlPanel, BorderLayout.NORTH)
            }
            add(browser.component, BorderLayout.CENTER)
        }

        // Create content with the main panel
        val content = ContentFactory.getInstance().createContent(mainPanel, null, false)
        toolWindow.contentManager.addContent(content)

        // NOTE: this must reload every time we receive a notification, because if we restart the language server
        // then the webview's connection to the old language server is dead.
        val busConnection = ApplicationManager.getApplication().messageBus.connect(toolWindow.disposable)
        busConnection.subscribe(
            BamlLanguageServerService.PORT_TOPIC,
            BamlLanguageServerService.PortListener { port ->
                thisLogger().info("received port notification $port before loadUrl")
                // Without this, it's possible for the user to open the tool window too fast
                Thread.sleep(500)
                browser.loadURL(BamlIdeConfig.getPlaygroundUrl(port))
                thisLogger().info("received port notification $port after loadUrl")
            }
        )

        Disposer.register(toolWindow.disposable, browser)
    }

    override fun shouldBeAvailable(project: Project) = true
}