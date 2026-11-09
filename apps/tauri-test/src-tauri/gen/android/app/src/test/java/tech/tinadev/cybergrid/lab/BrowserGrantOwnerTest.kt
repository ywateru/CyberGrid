package tech.tinadev.cybergrid.lab

import org.junit.Assert.*
import org.junit.Test

class BrowserGrantOwnerTest {
    private class Fixture {
        val events = mutableListOf<String>()
        var latest: String? = null
        var signIns = 0
        var resources = 0
        var clears = 0
        var finished = 0
        var failed = 0
        var initialAck = "Staged"
        var finalAck = "OwnedPersisted"
        var abortAck = "Cancelled"
        var stageThrows = false
        var cleanupOnly = false
        val cleanupSnapshots = mutableListOf<Boolean>()
        lateinit var signInCompletion: (Boolean) -> Unit
        lateinit var resourceCompletion: (Boolean) -> Unit
        val owner = BrowserGrantOwner("transaction", object : BrowserGrantOwner.Credentials {
            override fun signIn(completed: (Boolean) -> Unit) {
                signIns++; events += "signIn"; signInCompletion = completed
            }
            override fun resourceToken(completed: (Boolean) -> Unit) {
                resources++; events += "resource"; resourceCompletion = completed
            }
            override fun hasGrant() = latest != null
            override fun snapshot(forCleanup: Boolean): String {
                cleanupSnapshots += forCleanup
                check(forCleanup || !cleanupOnly)
                return requireNotNull(latest)
            }
            override fun clearAfterOwnershipAck() { clears++; latest = null; events += "clear" }
        }, object : BrowserGrantOwner.Native {
            override fun stage(tokens: String, finalGrant: Boolean): String {
                events += "stage:$finalGrant:$tokens"
                if (stageThrows) error("fixture response must never leave owner")
                return if (finalGrant) finalAck else initialAck
            }
            override fun abort(tokens: String): String {
                events += "abort:$tokens"; return abortAck
            }
            override fun failed() { failed++; events += "failed" }
        }, dispatch = { it() }, finished = { finished++ })
        fun signedIn() { latest = "initial"; signInCompletion(true) }
        fun resourceReady() { latest = "rotated"; resourceCompletion(true) }
    }

    @Test fun checkpointPrecedesResourceAndFinalOwnsLatestRotation() {
        val f = Fixture(); f.owner.start(); f.signedIn(); f.resourceReady()
        assertEquals(listOf("signIn", "stage:false:initial", "resource", "stage:true:rotated", "clear"), f.events)
        assertEquals(1, f.finished); assertEquals(1, f.clears); assertNull(f.latest)
    }

    @Test fun initialRejectedRetainsGrantAndOnlyRetriesCheckpoint() {
        val f = Fixture(); f.initialAck = "Rejected"; f.owner.start(); f.signedIn()
        assertEquals(0, f.resources); assertEquals("initial", f.latest); assertEquals(0, f.finished)
        f.initialAck = "Staged"; f.owner.retryCheckpoint()
        assertEquals(1, f.signIns); assertEquals(1, f.resources); assertEquals(0, f.clears)
    }

    @Test fun finalRejectedRetryNeverRefreshesAgain() {
        val f = Fixture(); f.finalAck = "Rejected"; f.owner.start(); f.signedIn(); f.resourceReady()
        assertEquals("rotated", f.latest); assertEquals(0, f.clears)
        f.finalAck = "OwnedPersisted"; f.owner.retryCheckpoint()
        assertEquals(1, f.resources); assertEquals(1, f.signIns); assertEquals(1, f.clears)
        assertEquals(2, f.events.count { it == "stage:true:rotated" })
    }

    @Test fun cancelDuringSignInWaitsForLateGrantAndAborts() {
        val f = Fixture(); f.owner.start(); f.owner.cancel()
        assertEquals(0, f.failed); assertEquals(0, f.clears); assertEquals(0, f.finished)
        f.signedIn()
        assertEquals(listOf("signIn", "abort:initial", "clear"), f.events)
        assertEquals(0, f.resources); assertEquals(1, f.finished)
    }

    @Test fun cancelDuringResourceWaitsForLatestRotatedGrant() {
        val f = Fixture(); f.owner.start(); f.signedIn(); f.owner.cancel()
        assertEquals(0, f.failed); assertEquals(0, f.clears)
        f.resourceReady()
        assertTrue(f.events.contains("abort:rotated"))
        assertFalse(f.events.any { it.startsWith("stage:true") }); assertEquals(1, f.clears)
    }

    @Test fun resourceFailureAbortsInitialGrantWithoutFinalImport() {
        val f = Fixture(); f.owner.start(); f.signedIn(); f.resourceCompletion(false)
        assertEquals(0, f.failed); assertTrue(f.events.contains("abort:initial"))
        assertFalse(f.events.any { it.startsWith("stage:true") }); assertEquals(1, f.clears)
    }

    @Test fun failureWithoutGrantUsesFailedOnlyAfterSdkSettles() {
        val f = Fixture(); f.owner.start(); f.owner.cancel()
        assertEquals(0, f.failed); f.signInCompletion(false)
        assertEquals(listOf("signIn", "failed"), f.events)
        assertEquals(1, f.finished); assertEquals(0, f.clears)
    }

    @Test fun abortRejectedRetainsLatestUntilCleanupAck() {
        val f = Fixture(); f.abortAck = "Rejected"; f.owner.start(); f.owner.cancel(); f.signedIn()
        assertEquals("initial", f.latest); assertEquals(0, f.finished); assertEquals(0, f.failed)
        f.abortAck = "Cancelled"; f.owner.retryCheckpoint()
        assertEquals(1, f.signIns); assertEquals(0, f.resources); assertEquals(1, f.clears)
    }

    @Test fun ownedPendingStopsSdkAndClearsOnlyAfterAck() {
        val f = Fixture(); f.finalAck = "OwnedPending"; f.owner.start(); f.signedIn(); f.resourceReady()
        f.owner.retryCheckpoint(); f.owner.cancel()
        assertEquals(1, f.clears); assertEquals(1, f.finished); assertEquals(1, f.resources)
    }

    @Test fun missingOrUnexpectedAckCannotDropGrantOrRefresh() {
        val f = Fixture(); f.stageThrows = true; f.owner.start(); f.signedIn()
        assertEquals("initial", f.latest); assertEquals(0, f.resources); assertEquals(0, f.finished)
        f.stageThrows = false; f.initialAck = "unexpected"; f.owner.retryCheckpoint()
        assertEquals("initial", f.latest); assertEquals(0, f.resources); assertEquals(0, f.clears)
    }

    @Test fun duplicateSdkCompletionCannotReplayStagingOrRefresh() {
        val f = Fixture(); f.owner.start(); f.signedIn(); f.signInCompletion(true)
        assertEquals(1, f.resources); assertEquals(1, f.events.count { it == "stage:false:initial" })
        f.resourceReady(); f.resourceCompletion(true)
        assertEquals(1, f.finished); assertEquals(1, f.clears)
    }

    @Test fun refreshOnlyGrantCanAbortButCannotBeImported() {
        val f = Fixture(); f.owner.start(); f.signedIn()
        f.cleanupOnly = true; f.resourceReady()
        assertEquals("rotated", f.latest); assertEquals(0, f.clears)
        assertFalse(f.events.any { it.startsWith("stage:true") })
        f.owner.cancel()
        assertEquals(true, f.cleanupSnapshots.last())
        assertTrue(f.events.contains("abort:rotated")); assertEquals(1, f.clears)
    }
}
