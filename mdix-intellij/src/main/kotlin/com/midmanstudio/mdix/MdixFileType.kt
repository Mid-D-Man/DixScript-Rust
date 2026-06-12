package com.midmanstudio.mdix

import com.intellij.lang.Language
import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object MdixFileType : LanguageFileType(MdixLanguage) {
    override fun getName()             = "MDIX"
    override fun getDescription()      = "DixScript Configuration File"
    override fun getDefaultExtension() = "mdix"
    override fun getIcon(): Icon?      = null  // swap in your SVG icon later
}
