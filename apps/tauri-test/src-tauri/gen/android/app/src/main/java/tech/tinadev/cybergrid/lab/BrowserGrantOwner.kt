package tech.tinadev.cybergrid.lab

/** Serialized grant handoff policy, independent of Android/SDK scheduling. */
internal class BrowserGrantOwner(
    val transactionId: String,
    private val credentials: Credentials,
    private val native: Native,
    private val dispatch: (() -> Unit) -> Unit,
    private val finished: () -> Unit,
) {
    interface Credentials {
        fun signIn(completed: (Boolean) -> Unit)
        fun resourceToken(completed: (Boolean) -> Unit)
        fun hasGrant(): Boolean
        fun snapshot(forCleanup: Boolean): String
        fun clearAfterOwnershipAck()
    }

    interface Native {
        fun stage(tokens: String, finalGrant: Boolean): String
        fun abort(tokens: String): String
        fun failed()
    }

    private enum class Pending { INITIAL, FINAL, ABORT }
    private var inFlight = false
    private var cancelled = false
    private var pending: Pending? = null
    private var settled = false
    private var callId = 0L

    fun start() = sdkCall(false)

    fun cancel() {
        if (settled) return
        cancelled = true
        // Never clear credentials or release native's barrier before completion.
        if (!inFlight) failOrAbort()
    }

    fun retryCheckpoint() {
        if (settled || inFlight) return
        val kind = pending
        if (kind == null) failOrAbort()
        else checkpoint(if (cancelled) Pending.ABORT else kind)
    }

    private fun sdkCall(resource: Boolean) {
        check(!settled && !inFlight)
        inFlight = true
        val expected = ++callId
        val completion: (Boolean) -> Unit = { success ->
            dispatch {
                // Guard even against a duplicate completion from a failed launch.
                if (!settled && inFlight && expected == callId) {
                    inFlight = false
                    if (!success) failOrAbort()
                    else checkpoint(when {
                        cancelled -> Pending.ABORT
                        resource -> Pending.FINAL
                        else -> Pending.INITIAL
                    })
                }
            }
        }
        try {
            if (resource) credentials.resourceToken(completion) else credentials.signIn(completion)
        } catch (_: Exception) { completion(false) }
    }

    private fun failOrAbort() {
        check(!inFlight)
        if (credentials.hasGrant()) checkpoint(Pending.ABORT)
        else {
            try { native.failed() } catch (_: Exception) { return } catch (_: LinkageError) { return }
            settled = true
            finished()
        }
    }

    private fun checkpoint(kind: Pending) {
        check(!inFlight)
        pending = kind
        // Serialization/JNI/ACK failure retains the latest actual SDK owner.
        val ack = try {
            val tokens = credentials.snapshot(kind == Pending.ABORT)
            if (kind == Pending.ABORT) native.abort(tokens)
            else native.stage(tokens, kind == Pending.FINAL)
        } catch (_: Exception) { return } catch (_: LinkageError) { return }
        when {
            ack == "Cancelled" ||
                (kind == Pending.FINAL && ack in setOf("OwnedPersisted", "OwnedPending")) -> {
                credentials.clearAfterOwnershipAck()
                pending = null
                settled = true
                finished()
            }
            kind == Pending.INITIAL && ack == "Staged" -> {
                pending = null
                if (cancelled) checkpoint(Pending.ABORT) else sdkCall(true)
            }
            // Rejected/unknown ACK deliberately leaves pending + credentials alive.
            else -> Unit
        }
    }
}
