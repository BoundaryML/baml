This is the source code for the BAML IntelliJ extension, which provides
the following functionality:

- syntax highlighting for BAML source code
- language server functionality for BAML source code (xref navigation, jump-to-definition, etc)
- the BAML playground, which allows a user to interact with their BAML source code and run tests

It is implemented as follows:

- builds are managed with Gradle
- Kotlin source code for the extension
- LSP4IJ is a third-party framework used to provide LSP integration (this connects our LSP with Jetbrains)
    - This is a new, actively developed framework documented at https://github.com/redhat-developer/lsp4ij/blob/main/docs/DeveloperGuide.md
- The BAML playground is shown with BamlToolWindowFactory.kt
    - It relies on Intellij's JCEF (Java Chromium Embedded Framework) integration, which is documented at https://plugins.jetbrains.com/docs/intellij/embedded-browser-jcef.html

This directory was originally created using https://github.com/JetBrains/intellij-platform-plugin-template