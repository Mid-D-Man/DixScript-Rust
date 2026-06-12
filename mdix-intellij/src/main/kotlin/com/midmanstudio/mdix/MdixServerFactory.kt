package com.midmanstudio.mdix

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.client.LanguageClientImpl
import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import java.io.File
import java.nio.file.Paths

class MdixServerFactory : LanguageServerFactory {

    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        val binary = resolveBinary()
        return ProcessStreamConnectionProvider(listOf(binary), System.getenv())
    }

    override fun createLanguageClient(project: Project): LanguageClientImpl {
        return LanguageClientImpl(project)
    }

    // ── Binary resolution ─────────────────────────────────────────────────────

    private fun resolveBinary(): String {
        val exe = if (System.getProperty("os.name").lowercase().contains("win"))
            "mdix-lsp.exe" else "mdix-lsp"

        // 1. Env override
        System.getenv("MDIX_LSP_PATH")
            ?.takeIf { File(it).exists() }
            ?.let { return it }

        // 2. Bundled inside the plugin JAR's sibling bin/ directory
        val pluginDir  = Paths.get(
            MdixServerFactory::class.java.protectionDomain.codeSource.location.toURI()
        ).parent.parent.toString()
        val bundled = File(pluginDir, "bin/${platformDir()}/$exe")
        if (bundled.exists()) return bundled.absolutePath

        // 3. System PATH
        which(exe)?.let { return it }

        error(
            "mdix-lsp binary not found. " +
            "Build with `cargo build -p mdix-lsp --release` and set MDIX_LSP_PATH, " +
            "or place the binary in the plugin's bin/ directory."
        )
    }

    private fun platformDir(): String {
        val os   = System.getProperty("os.name").lowercase()
        val arch = System.getProperty("os.arch").lowercase()
        return when {
            os.contains("mac")   && arch.contains("aarch64") -> "darwin-arm64"
            os.contains("mac")                               -> "darwin-x64"
            os.contains("win")                               -> "win32-x64"
            arch.contains("aarch64")                         -> "linux-arm64"
            else                                             -> "linux-x64"
        }
    }

    private fun which(name: String): String? = try {
        val cmd = if (System.getProperty("os.name").lowercase().contains("win"))
            arrayOf("where", name) else arrayOf("which", name)
        Runtime.getRuntime().exec(cmd)
            .inputStream.bufferedReader().readLine()
            ?.trim()
            ?.takeIf { File(it).exists() }
    } catch (_: Exception) { null }
}
