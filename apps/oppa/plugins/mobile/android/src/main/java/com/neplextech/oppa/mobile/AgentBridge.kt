package com.neplextech.oppa.mobile

import android.app.Activity
import app.tauri.plugin.Channel
import app.tauri.plugin.JSObject
import java.lang.ref.WeakReference

/**
 * Process-wide link between native components and the Rust agent runtime.
 *
 * The Rust runtime lives as long as the process, independently of the
 * activity. It registers [channel] once; native components such as the
 * foreground service send events through it without touching the activity.
 */
internal object AgentBridge {
    @Volatile
    private var channel: Channel? = null

    /** Whether a Rust runtime registered its event channel in this process. */
    val runtimeAttached: Boolean
        get() = channel != null

    @Volatile
    private var current: WeakReference<Activity>? = null

    fun attach(handler: Channel) {
        channel = handler
    }

    /**
     * The newest live activity. Tauri binds plugins to the first activity of
     * the process, which is destroyed when the user swipes the task away while
     * the foreground service keeps the process alive.
     */
    val activity: Activity?
        get() = current?.get()?.takeUnless { it.isFinishing || it.isDestroyed }

    fun activityCreated(activity: Activity) {
        current = WeakReference(activity)
        send("activityAvailable")
    }

    fun send(type: String) {
        val handler = channel ?: return
        try {
            handler.send(JSObject().put("type", type))
        } catch (error: Exception) {
            Logs.warn("event $type could not be delivered: ${error.javaClass.simpleName}")
        }
    }
}

/**
 * Entry point for the host activity. `MainActivity` reports every new
 * instance so the Rust host can rebuild its web view window when Android
 * recreates the activity in a process the foreground service kept alive.
 */
object OppaActivityHost {
    /** Call from `onCreate` after `super.onCreate` registered the activity with Tauri. */
    @JvmStatic
    fun onActivityCreated(activity: Activity) {
        AgentBridge.activityCreated(activity)
    }
}

internal object Logs {
    private const val TAG = "OppaMobile"

    fun warn(message: String) {
        android.util.Log.w(TAG, message)
    }

    fun info(message: String) {
        android.util.Log.i(TAG, message)
    }
}
