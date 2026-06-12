package com.midmanstudio.mdix

import com.intellij.openapi.util.IconLoader
import javax.swing.Icon

object MdixIcons {
    // IconLoader automatically serves mdix-file_dark.svg when a dark theme is active.
    @JvmField
    val FILE: Icon = IconLoader.getIcon("/icons/mdix-file.svg", MdixIcons::class.java)
}
