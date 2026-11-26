package com.neplextech.oppa.mobile

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.FileProvider
import java.io.File
import java.security.MessageDigest

/**
 * Hands a downloaded update APK to the system package installer.
 *
 * Before the installer is shown, the archive must declare this app's package
 * name, a higher version code, and exactly the signing certificates of the
 * installed app. Android enforces the certificate match again at install
 * time; checking first gives the user a clear error instead of a generic
 * installer failure. Installation always requires the user's approval.
 */
internal object ApkInstaller {
    fun install(activity: Activity, path: String) {
        val context = activity.applicationContext
        val updates = File(context.cacheDir, UPDATE_DIRECTORY).canonicalFile
        val file = File(path).canonicalFile
        require(file.parentFile == updates && file.isFile) {
            "update file is outside the update cache"
        }
        val archive = archiveInfo(context, file)
            ?: throw IllegalStateException("the downloaded file is not a valid APK")
        require(archive.packageName == context.packageName) {
            "the update is for a different application"
        }
        val installed = installedInfo(context)
        require(versionCode(archive) > versionCode(installed)) {
            "the update is not newer than the installed version"
        }
        require(certificateDigests(archive) == certificateDigests(installed)) {
            "the update is not signed with this application's signing key"
        }

        val uri = FileProvider.getUriForFile(context, "${context.packageName}.oppa.updates", file)
        val intent = Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, "application/vnd.android.package-archive")
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.startActivity(intent)
    }

    fun canRequestInstalls(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.O ||
            context.packageManager.canRequestPackageInstalls()

    private fun archiveInfo(context: Context, file: File): PackageInfo? {
        val manager = context.packageManager
        return if (Build.VERSION.SDK_INT >= 33) {
            manager.getPackageArchiveInfo(
                file.path,
                PackageManager.PackageInfoFlags.of(PackageManager.GET_SIGNING_CERTIFICATES.toLong())
            )
        } else if (Build.VERSION.SDK_INT >= 28) {
            @Suppress("DEPRECATION")
            manager.getPackageArchiveInfo(file.path, PackageManager.GET_SIGNING_CERTIFICATES)
        } else {
            @Suppress("DEPRECATION")
            manager.getPackageArchiveInfo(file.path, PackageManager.GET_SIGNATURES)
        }
    }

    private fun installedInfo(context: Context): PackageInfo {
        val manager = context.packageManager
        return if (Build.VERSION.SDK_INT >= 33) {
            manager.getPackageInfo(
                context.packageName,
                PackageManager.PackageInfoFlags.of(PackageManager.GET_SIGNING_CERTIFICATES.toLong())
            )
        } else if (Build.VERSION.SDK_INT >= 28) {
            @Suppress("DEPRECATION")
            manager.getPackageInfo(context.packageName, PackageManager.GET_SIGNING_CERTIFICATES)
        } else {
            @Suppress("DEPRECATION")
            manager.getPackageInfo(context.packageName, PackageManager.GET_SIGNATURES)
        }
    }

    private fun versionCode(info: PackageInfo): Long =
        if (Build.VERSION.SDK_INT >= 28) {
            info.longVersionCode
        } else {
            @Suppress("DEPRECATION")
            info.versionCode.toLong()
        }

    private fun certificateDigests(info: PackageInfo): Set<String> {
        val signatures = if (Build.VERSION.SDK_INT >= 28) {
            val signing = info.signingInfo ?: return emptySet()
            if (signing.hasMultipleSigners()) {
                signing.apkContentsSigners
            } else {
                signing.signingCertificateHistory
            }
        } else {
            @Suppress("DEPRECATION")
            info.signatures
        } ?: return emptySet()
        val digest = MessageDigest.getInstance("SHA-256")
        return signatures.map { signature ->
            digest.digest(signature.toByteArray()).joinToString("") { "%02x".format(it) }
        }.toSet().also {
            require(it.isNotEmpty()) { "the APK has no signing certificate" }
        }
    }

    const val UPDATE_DIRECTORY = "updates"
}
