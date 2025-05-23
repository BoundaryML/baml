package com.boundaryml.jetbrains_ext

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

/**
 * Definition of the BAML file type.
 *
 * @see [Language and File Type](https://plugins.jetbrains.com/docs/intellij/language-and-filetype.html)
 */
class BamlFileType private constructor() : LanguageFileType(BamlLanguage) {

    companion object {
        val INSTANCE: BamlFileType = BamlFileType()
    }

    /**
     * Returns the name of the file type. The name must be unique among all file types registered in the system.
     */
    override fun getName(): String = "BAML"

    /**
     * Returns the user-readable description of the file type.
     */
    override fun getDescription(): String = "BAML file"

    /**
     * Returns the default extension for files of the type, *not* including the leading '.'.
     */
    override fun getDefaultExtension(): String = "baml"

    /**
     * Returns the icon used for showing files of the type, or `null` if no icon should be shown.
     */
    override fun getIcon(): Icon? = BamlIcons.FILETYPE
}
