package tech.tinadev.cybergrid.lab

import android.net.Uri
import io.logto.sdk.android.LogtoClient
import io.logto.sdk.android.type.AccessToken
import io.logto.sdk.android.type.LogtoConfig
import org.json.JSONObject
import java.util.concurrent.Executors

/**
 * A process-local owner, retained across Activity recreation and failed ACKs.
 * All SDK completions, protected-field reads, native checkpoints and disposal are
 * serialized here. SDK calls start on the UI thread, one at a time. We never use
 * signOut/clearCredentials: either could revoke the grant just handed to Rust.
 */
internal object BrowserLoginBridge {
    private const val REDIRECT = "tech.tinadev.cybergrid.lab://tech.tinadev.cybergrid.lab/callback"
    private val worker = Executors.newSingleThreadExecutor { task ->
        Thread(task, "cybergrid-browser-owner").apply { isDaemon = true }
    }
    private var active: BrowserGrantOwner? = null // accessed only by worker

    fun start(activity: MainActivity, transactionId: String, configJson: String) {
        worker.execute {
            val previous = active
            if (previous != null) {
                // A repeat only retries the unacknowledged checkpoint; no SDK call
                // is repeated and a retained grant cannot be replaced by a new one.
                if (previous.transactionId == transactionId) previous.retryCheckpoint()
                else safeFailed(activity, transactionId)
                return@execute
            }
            val config = try { Config.parse(configJson) } catch (_: Exception) {
                safeFailed(activity, transactionId)
                return@execute
            }
            val client = try { HandoffClient(config, activity) } catch (_: Exception) {
                safeFailed(activity, transactionId)
                return@execute
            }
            val session = BrowserGrantOwner(
                transactionId, client,
                object : BrowserGrantOwner.Native {
                    override fun stage(tokens: String, finalGrant: Boolean) =
                        activity.browserStage(transactionId, tokens, finalGrant)
                    override fun abort(tokens: String) = activity.browserAbort(transactionId, tokens)
                    override fun failed() = activity.browserFailed(transactionId)
                },
                dispatch = { action -> worker.execute(action) },
                finished = { active = null },
            )
            active = session
            session.start()
        }
    }

    fun cancel(transactionId: String) {
        worker.execute {
            active?.takeIf { it.transactionId == transactionId }?.cancel()
        }
    }

    private fun safeFailed(activity: MainActivity, transactionId: String) {
        try { activity.browserFailed(transactionId) } catch (_: Exception) { /* No grant exists. */ }
        catch (_: LinkageError) { /* No grant exists. */ }
    }

    private data class Config(
        val endpoint: String, val appId: String, val resource: String,
        val scope: List<String>, val redirectUri: String,
    ) {
        companion object {
            fun parse(text: String): Config {
                require(text.toByteArray(Charsets.UTF_8).size <= 8192)
                val json = JSONObject(text)
                fun field(name: String): String {
                    val value = json.get(name)
                    require(value is String && value.isNotBlank() && value.length <= 4096)
                    return value
                }
                val endpoint = field("endpoint")
                val uri = Uri.parse(endpoint)
                require(uri.scheme == "https" && !uri.host.isNullOrBlank() && uri.userInfo == null)
                val redirect = field("redirectUri")
                require(redirect == REDIRECT)
                val scope = field("scope").split(' ').filter { it.isNotBlank() }.distinct()
                require(scope.contains("openid") && scope.contains("offline_access"))
                return Config(endpoint, field("appId"), field("resource"), scope, redirect)
            }
        }
    }

    /** Fixed 3.0.0 protected extension. No reflection or SDK disk storage. */
    private class HandoffClient(private val config: Config, private val activity: MainActivity) : LogtoClient(
        LogtoConfig(
            endpoint = config.endpoint, appId = config.appId,
            scopes = config.scope, resources = listOf(config.resource),
            usingPersistStorage = false, includeReservedScopes = false,
        ), activity.application,
    ), BrowserGrantOwner.Credentials {
        private var preferred: AccessToken? = null

        override fun signIn(completed: (Boolean) -> Unit) {
            activity.runOnUiThread {
                try { signIn(activity, config.redirectUri) { error -> completed(error == null) } }
                catch (_: Exception) { completed(false) }
            }
        }

        override fun resourceToken(completed: (Boolean) -> Unit) {
            activity.runOnUiThread {
                try {
                    getAccessToken(config.resource) { error, access ->
                        // Keep the preferred access token alongside protected latest RT;
                        // the owner reads it only after the serial completion handoff.
                        if (error == null && access != null) preferred = access
                        completed(error == null && access != null)
                    }
                } catch (_: Exception) { completed(false) }
            }
        }

        override fun hasGrant(): Boolean = refreshToken != null || idToken != null || accessTokenMap.isNotEmpty()

        override fun snapshot(forCleanup: Boolean): String {
            val refresh = requireNotNull(refreshToken).also { require(it.isNotBlank()) }
            val access = preferred ?: accessTokenMap.values.maxByOrNull { it.expiresAt }
            // Cleanup journals only require the latest refresh token. Missing AT
            // must never make a final import succeed or discard an owned grant.
            require(forCleanup || access != null)
            return JSONObject().apply {
                put("accessToken", access?.token ?: "")
                put("refreshToken", refresh)
                idToken?.let { put("idToken", it) }
                put("expiresIn", access?.let { (it.expiresAt - System.currentTimeMillis() / 1000).coerceAtLeast(1) } ?: 0)
                put("tokenType", if (access != null) "Bearer" else "")
                put("scope", access?.scope ?: "")
            }.toString()
        }

        override fun clearAfterOwnershipAck() {
            // Only called after every SDK call has settled and Rust ACKed ownership.
            refreshToken = null
            idToken = null
            accessTokenMap.clear()
            preferred = null
        }
    }
}
