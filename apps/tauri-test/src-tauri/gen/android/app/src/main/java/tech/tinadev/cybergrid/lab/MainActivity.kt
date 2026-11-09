package tech.tinadev.cybergrid.lab

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.annotation.Keep

class MainActivity : TauriActivity() {
  // Called from Rust/JNI. OAuth credentials never enter the WebView or ADB DTOs.
  @Keep
  fun startBrowserLogin(transactionId: String, configJson: String) {
    runOnUiThread { BrowserLoginBridge.start(this, transactionId, configJson) }
  }

  @Keep
  fun cancelBrowserLogin(transactionId: String) {
    BrowserLoginBridge.cancel(transactionId)
  }

  @Keep
  external fun browserStage(transactionId: String, tokensJson: String, finalGrant: Boolean): String

  @Keep
  external fun browserAbort(transactionId: String, tokensJson: String): String

  @Keep
  external fun browserFailed(transactionId: String)

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
