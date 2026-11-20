package garden.tiny.widget;

import static org.junit.Assert.*;
import org.junit.Test;

/** Exercises uncertain outcomes, cached state and deferred work; no simulated rewards. */
public final class CarePolicyTest {
    @Test public void unchangedLeaseRevisionAndSequenceCannotResolvePendingIntent() {
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,42,"lease-a",3));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,42,"lease-a",0));
    }

    @Test public void committedCounterFencesDuplicateWithoutClaimingCareSucceeded() {
        assertTrue(CarePolicy.pendingIsFenced(42,"lease-a",4,43,"lease-a",4));
        assertTrue(CarePolicy.pendingIsFenced(42,"lease-a",4,44,"lease-a",5));
    }

    @Test public void unrelatedDurableRevisionChangeMakesOriginalRequestStale() {
        // Another change can advance revision without processing this care. The
        // old request cannot then commit, but this is not a successful-care receipt.
        assertTrue(CarePolicy.pendingIsFenced(42,"lease-a",4,43,"lease-a",3));
    }

    @Test public void releasedOrReplacedLeaseFencesOldGeneration() {
        assertTrue(CarePolicy.pendingIsFenced(42,"lease-a",4,44,"lease-b",0));
        assertTrue(CarePolicy.pendingIsFenced(42,"lease-a",4,44,null,-1));
    }

    @Test public void rollbackCannotReconcilePendingEvenWithDifferentLease() {
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,41,"lease-b",0));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,41,null,-1));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,41,"lease-a",4));
    }

    @Test public void malformedCountersAndLeaseSentinelsDoNotClearPending() {
        long unsafe=CarePolicy.MAX_SAFE_REVISION+1;
        assertFalse(CarePolicy.pendingIsFenced(-1,"lease-a",4,43,"lease-a",4));
        assertFalse(CarePolicy.pendingIsFenced(unsafe,"lease-a",4,unsafe,"lease-a",4));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,unsafe,"lease-a",4));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",0,43,"lease-a",4));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",unsafe,43,"lease-a",4));
        assertFalse(CarePolicy.pendingIsFenced(42,null,4,43,"lease-a",4));
        assertFalse(CarePolicy.pendingIsFenced(42," ",4,43,"lease-a",4));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,43,"lease-a",-1));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,43,"lease-a",unsafe));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,43,"",4));
        assertFalse(CarePolicy.pendingIsFenced(42,"lease-a",4,43,null,0));
    }

    @Test public void firstProjectionMustRespectPairingBaselineAndCanonicalChao() {
        assertTrue(CarePolicy.acceptsSnapshot("chao", 42, -1, "chao", 42));
        assertTrue(CarePolicy.acceptsSnapshot("chao", 42, -1, "chao", 43));
        assertFalse(CarePolicy.acceptsSnapshot("chao", 42, -1, "chao", 41));
        assertFalse(CarePolicy.acceptsSnapshot("chao", 42, -1, "other", 43));
        assertFalse(CarePolicy.acceptsSnapshot(null, 42, -1, null, 43));
        assertFalse(CarePolicy.acceptsSnapshot("", 42, -1, "", 43));
    }

    @Test public void refreshCanRepeatButCannotRollBackConfirmedRevision() {
        assertTrue(CarePolicy.acceptsSnapshot("chao", 42, 44, "chao", 44));
        assertTrue(CarePolicy.acceptsSnapshot("chao", 42, 44, "chao", 45));
        assertFalse(CarePolicy.acceptsSnapshot("chao", 42, 44, "chao", 43));
        assertFalse(CarePolicy.acceptsSnapshot("chao", 42, -2, "chao", 44));
    }

    @Test public void deferredFirstDeliveryExpiresWithoutNewUserIntent() {
        assertTrue(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-a", 1000, 1000));
        assertTrue(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-a", 1000, 16000));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-a", 1000, 16001));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-a", 1000, 3_601_000));
    }

    @Test public void processDeathRetryAndInvalidMonotonicTimeRejectQueuedMutation() {
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-b", 1000, 1001));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(2, "process-a", "process-a", 1000, 1001));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(0, "process-a", "process-a", 1000, 1001));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-a", 1001, 1000));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-a", -1, 1000));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, null, null, 1000, 1001));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, "", "", 1000, 1001));
        assertFalse(CarePolicy.firstDeliveryIsCurrent(1, "process-a", "process-a", 1, Long.MAX_VALUE));
    }

    @Test public void unsafeRevisionsFailClosedInEveryReconciliationPath() {
        long unsafe = CarePolicy.MAX_SAFE_REVISION + 1;
        assertFalse(CarePolicy.acceptsSnapshot("chao", unsafe, -1, "chao", unsafe));
        assertFalse(CarePolicy.acceptsSnapshot("chao", 42, -1, "chao", unsafe));
        assertFalse(CarePolicy.acceptsSnapshot("chao", 42, unsafe, "chao", unsafe));
    }
}
