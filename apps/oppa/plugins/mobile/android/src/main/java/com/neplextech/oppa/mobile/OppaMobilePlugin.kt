package com.neplextech.oppa.mobile

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.Network
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.webkit.WebView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.app.NotificationManagerCompat
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class RegisterEventsArgs {
    lateinit var handler: Channel
}

@InvokeArg
class AccountArgs {
    lateinit var account: String
}

@InvokeArg
class CredentialSetArgs {
    lateinit var account: String
    lateinit var value: String
}

@InvokeArg
class BackgroundServiceArgs {
    var enabled: Boolean = false
    lateinit var statusPath: String
}

@InvokeArg
class BrowseArgs {
    var serviceTypes: List<String> = listOf()
    var timeoutMs: Long = 4000
}

@InvokeArg
class UrlArgs {
    lateinit var url: String
}

@InvokeArg
class ShareArgs {
    lateinit var title: String
    lateinit var text: String
}

@InvokeArg
class PathArgs {
    lateinit var path: String
}

/**
 * Native bridge for the OPPA agent. Only Rust calls these commands; none are
 * exposed to the frontend. All state lives in application-scoped objects so
 * the agent keeps working after the activity is destroyed.
 */
@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications")
    ]
)
class OppaMobilePlugin(private val activity: Activity) : Plugin(activity) {
    private val context: Context = activity.applicationContext
    private val credentials by lazy { KeystoreCredentials(context) }

    override fun load(webView: WebView) {
        super.load(webView)
        registerNetworkCallback(context)
        AgentBridge.send("activityAvailable")
    }

    override fun onResume() {
        super.onResume()
        AgentBridge.send("activityAvailable")
        AgentBridge.send("foregrounded")
        // Starting a foreground service is only permitted while visible.
        startServiceIfWanted()
        // Permission may have been granted in system settings meanwhile.
        AgentForegroundService.refreshNotification(context)
    }

    override fun onPause() {
        super.onPause()
        AgentBridge.send("backgrounded")
    }

    override fun onDestroy(activity: AppCompatActivity) {
        super.onDestroy(activity)
        // A recreated activity reports `activityAvailable` again when it resumes.
        AgentBridge.send("activityDestroyed")
    }

    @Command
    fun registerEvents(invoke: Invoke) {
        val args = invoke.parseArgs(RegisterEventsArgs::class.java)
        AgentBridge.attach(args.handler)
        invoke.resolve()
    }

    @Command
    fun credentialGet(invoke: Invoke) {
        val args = invoke.parseArgs(AccountArgs::class.java)
        try {
            val result = JSObject()
            result.put("value", credentials.get(args.account))
            invoke.resolve(result)
        } catch (error: Exception) {
            invoke.reject("secure credential could not be read: ${error.javaClass.simpleName}")
        }
    }

    @Command
    fun credentialSet(invoke: Invoke) {
        val args = invoke.parseArgs(CredentialSetArgs::class.java)
        try {
            credentials.set(args.account, args.value)
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject("secure credential could not be stored: ${error.javaClass.simpleName}")
        }
    }

    @Command
    fun credentialDelete(invoke: Invoke) {
        val args = invoke.parseArgs(AccountArgs::class.java)
        try {
            credentials.delete(args.account)
            invoke.resolve()
        } catch (error: Exception) {
            invoke.reject("secure credential could not be deleted: ${error.javaClass.simpleName}")
        }
    }

    @Command
    fun setBackgroundService(invoke: Invoke) {
        val args = invoke.parseArgs(BackgroundServiceArgs::class.java)
        AgentForegroundService.preferences(context).edit()
            .putBoolean(AgentForegroundService.KEY_ENABLED, args.enabled)
            .putString(AgentForegroundService.KEY_STATUS_PATH, args.statusPath)
            .apply()
        if (args.enabled) {
            startServiceIfWanted()
        } else {
            AgentForegroundService.stop(context)
        }
        invoke.resolve()
    }

    @Command
    fun ensureBackgroundService(invoke: Invoke) {
        startServiceIfWanted()
        invoke.resolve()
    }

    @Command
    fun backgroundStatus(invoke: Invoke) {
        val power = context.getSystemService(Context.POWER_SERVICE) as PowerManager
        val result = JSObject()
        result.put(
            "enabled",
            AgentForegroundService.preferences(context)
                .getBoolean(AgentForegroundService.KEY_ENABLED, false)
        )
        result.put("running", AgentForegroundService.running)
        result.put("notificationsGranted", NotificationManagerCompat.from(context).areNotificationsEnabled())
        result.put(
            "ignoringBatteryOptimizations",
            Build.VERSION.SDK_INT < Build.VERSION_CODES.M ||
                power.isIgnoringBatteryOptimizations(context.packageName)
        )
        result.put("manufacturer", Build.MANUFACTURER ?: "")
        result.put("sdkInt", Build.VERSION.SDK_INT)
        invoke.resolve(result)
    }

    @Command
    fun requestNotificationPermission(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < 33 ||
            getPermissionState("notifications") == PermissionState.GRANTED
        ) {
            resolveBoolean(invoke, NotificationManagerCompat.from(context).areNotificationsEnabled())
            return
        }
        requestPermissionForAlias("notifications", invoke, "notificationPermissionResult")
    }

    @PermissionCallback
    private fun notificationPermissionResult(invoke: Invoke) {
        val granted = getPermissionState("notifications") == PermissionState.GRANTED
        if (granted) AgentForegroundService.refreshNotification(context)
        resolveBoolean(invoke, granted)
    }

    @SuppressLint("BatteryLife")
    @Command
    fun requestBatteryExemption(invoke: Invoke) {
        // Direct distribution permits the request dialog; fall back to the
        // system list when a vendor build does not handle it.
        val direct = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
            .setData(Uri.parse("package:${context.packageName}"))
        if (!launch(direct)) {
            launch(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
        }
        invoke.resolve()
    }

    @Command
    fun openAppSettings(invoke: Invoke) {
        launch(
            Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
                .setData(Uri.parse("package:${context.packageName}"))
        )
        invoke.resolve()
    }

    @Command
    fun browseServices(invoke: Invoke) {
        val args = invoke.parseArgs(BrowseArgs::class.java)
        val allowed = args.serviceTypes.filter { SERVICE_TYPE.matches(it) }.take(8)
        ServiceBrowser(context).browse(allowed, args.timeoutMs) { endpoints ->
            val services = JSArray()
            for (endpoint in endpoints) {
                val item = JSObject()
                item.put("name", endpoint.name)
                item.put("serviceType", endpoint.serviceType)
                item.put("host", endpoint.host)
                item.put("port", endpoint.port)
                services.put(item)
            }
            val result = JSObject()
            result.put("services", services)
            invoke.resolve(result)
        }
    }

    @Command
    fun openUrl(invoke: Invoke) {
        val args = invoke.parseArgs(UrlArgs::class.java)
        val uri = Uri.parse(args.url)
        if (uri.scheme != "https" && uri.scheme != "http") {
            invoke.reject("only HTTP(S) URLs can be opened")
            return
        }
        if (launch(Intent(Intent.ACTION_VIEW, uri))) invoke.resolve() else invoke.reject("no browser is available")
    }

    @Command
    fun shareText(invoke: Invoke) {
        val args = invoke.parseArgs(ShareArgs::class.java)
        val send = Intent(Intent.ACTION_SEND)
            .setType("text/plain")
            .putExtra(Intent.EXTRA_SUBJECT, args.title)
            .putExtra(Intent.EXTRA_TEXT, args.text)
        if (launch(Intent.createChooser(send, args.title))) invoke.resolve() else invoke.reject("sharing is unavailable")
    }

    @Command
    fun canInstallPackages(invoke: Invoke) {
        resolveBoolean(invoke, ApkInstaller.canRequestInstalls(context))
    }

    @Command
    fun openInstallSettings(invoke: Invoke) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            launch(
                Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES)
                    .setData(Uri.parse("package:${context.packageName}"))
            )
        }
        invoke.resolve()
    }

    @Command
    fun installApk(invoke: Invoke) {
        val args = invoke.parseArgs(PathArgs::class.java)
        try {
            ApkInstaller.install(activity, args.path)
            invoke.resolve()
        } catch (error: IllegalArgumentException) {
            invoke.reject(error.message ?: "the update could not be verified")
        } catch (error: Exception) {
            invoke.reject("the installer could not be opened: ${error.javaClass.simpleName}")
        }
    }

    private fun startServiceIfWanted() {
        val preferences = AgentForegroundService.preferences(context)
        if (!preferences.getBoolean(AgentForegroundService.KEY_ENABLED, false)) return
        val paired = AgentForegroundService.readStatus(context)?.optBoolean("paired", false) ?: false
        if (!paired || !AgentBridge.runtimeAttached) return
        try {
            AgentForegroundService.start(context)
        } catch (error: Exception) {
            // ForegroundServiceStartNotAllowedException when not visible.
            Logs.warn("foreground service could not start: ${error.javaClass.simpleName}")
        }
    }

    /**
     * Tauri binds plugins to the first activity, which may since have been
     * recreated, so fall back to a new task from the application context.
     */
    private fun launch(intent: Intent): Boolean = try {
        activity.startActivity(intent)
        true
    } catch (error: Exception) {
        try {
            context.startActivity(Intent(intent).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            true
        } catch (fallback: Exception) {
            false
        }
    }

    private fun resolveBoolean(invoke: Invoke, value: Boolean) {
        val result = JSObject()
        result.put("value", value)
        invoke.resolve(result)
    }

    companion object {
        private val SERVICE_TYPE = Regex("^_[a-z0-9-]{1,32}\\._(tcp|udp)$")

        @Volatile
        private var networkCallbackRegistered = false

        /** Reports default-network changes for the life of the process. */
        @Synchronized
        private fun registerNetworkCallback(context: Context) {
            if (networkCallbackRegistered || Build.VERSION.SDK_INT < Build.VERSION_CODES.N) return
            val connectivity = context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager
            connectivity.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
                private var current: Network? = null
                private var initialized = false

                override fun onAvailable(network: Network) {
                    // The callback reports the existing network immediately on
                    // registration; only later changes are worth a reconnect.
                    val changed = initialized && current != network
                    initialized = true
                    current = network
                    if (changed) AgentBridge.send("networkAvailable")
                }

                override fun onLost(network: Network) {
                    initialized = true
                    if (current == network) {
                        current = null
                        AgentBridge.send("networkLost")
                    }
                }
            })
            networkCallbackRegistered = true
        }
    }
}
