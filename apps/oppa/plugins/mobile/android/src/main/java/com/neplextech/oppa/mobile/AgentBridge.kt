package com.neplextech.oppa.mobile

import app.tauri.plugin.Channel
import app.tauri.plugin.JSObject

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

    fun attach(handler: Channel) {
        channel = handler
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

internal object Logs {
    private const val TAG = "OppaMobile"

    fun warn(message: String) {
        android.util.Log.w(TAG, message)
    }

    fun info(message: String) {
        android.util.Log.i(TAG, message)
    }
}
