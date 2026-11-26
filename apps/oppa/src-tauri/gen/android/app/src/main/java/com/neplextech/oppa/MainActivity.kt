package com.neplextech.oppa

import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.core.content.ContextCompat
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.neplextech.oppa.mobile.OppaActivityHost

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Leave the branded launch theme before any content is created.
    setTheme(R.style.Theme_oppa)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    applySystemInsets(findViewById(android.R.id.content))
    // The process may outlive this activity (foreground service). Tell the
    // Rust host so it rebuilds the web view window for a recreated activity.
    OppaActivityHost.onActivityCreated(this)
  }

  /** Paints the theme background until the page renders, avoiding a white flash. */
  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    webView.setBackgroundColor(ContextCompat.getColor(this, R.color.oppa_background))
  }

  /**
   * Keeps the WebView clear of the status bar, navigation bar, display cutout,
   * and on-screen keyboard on every Android version. WebView support for CSS
   * safe-area insets varies by version, so insets are applied natively and
   * consumed here instead.
   */
  private fun applySystemInsets(content: View) {
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or
          WindowInsetsCompat.Type.displayCutout() or
          WindowInsetsCompat.Type.ime()
      )
      view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
      WindowInsetsCompat.CONSUMED
    }
  }
}
