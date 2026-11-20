package garden.tiny.widget;

/**
 * Display and dispatch policy only. This class grants no authority: Rust still
 * validates credentials, capabilities, activity ownership and durable receipts.
 * Kept free of Android and JSON dependencies so process/retry regressions can be
 * checked on the host JVM.
 */
public final class CarePolicy {
    public static final long MAX_QUEUE_AGE_MS = 15_000L;
    public static final long MAX_SAFE_REVISION = 9_007_199_254_740_991L;

    private CarePolicy() { }

    /**
     * True means the original fenced request cannot apply a new mutation later.
     * It does not claim whether the care effect happened. Revision advances can
     * come from an unrelated durable change; the old request is then stale.
     * The caller must use fresh canonical values and never automatically resend.
     */
    public static boolean pendingIsFenced(long requestedRevision,String requestedLease,long requestedSequence,
            long currentRevision,String currentLease,long lastSequence) {
        if(!revision(requestedRevision) || !revision(currentRevision) || currentRevision<requestedRevision
                || !present(requestedLease) || requestedSequence<1 || requestedSequence>MAX_SAFE_REVISION) return false;
        if(currentLease==null ? lastSequence!=-1 : !present(currentLease) || !revision(lastSequence)) return false;
        return currentRevision>requestedRevision || !requestedLease.equals(currentLease) || lastSequence>=requestedSequence;
    }

    /** cachedRevision is -1 only when no snapshot has yet been confirmed. */
    public static boolean acceptsSnapshot(String pairedChaoId, long pairedRevision,
            long cachedRevision, String incomingChaoId, long incomingRevision) {
        return present(pairedChaoId) && pairedChaoId.equals(incomingChaoId)
                && revision(pairedRevision) && cachedRevision(cachedRevision)
                && revision(incomingRevision) && incomingRevision >= pairedRevision
                && incomingRevision >= cachedRevision;
    }

    /**
     * Delivery count one alone does not prove a recent user gesture. Jobs can
     * wait offline for hours before their first delivery. A changed process also
     * loses the monotonic freshness established by that gesture.
     */
    public static boolean firstDeliveryIsCurrent(int deliveryCount, String queuedProcess,
            String currentProcess, long clickedElapsedMs, long nowElapsedMs) {
        return deliveryCount == 1 && present(queuedProcess) && queuedProcess.equals(currentProcess)
                && clickedElapsedMs >= 0 && nowElapsedMs >= clickedElapsedMs
                && nowElapsedMs - clickedElapsedMs <= MAX_QUEUE_AGE_MS;
    }

    private static boolean present(String value) {
        return value != null && !value.trim().isEmpty();
    }

    private static boolean revision(long value) {
        return value >= 0 && value <= MAX_SAFE_REVISION;
    }

    private static boolean cachedRevision(long value) {
        return value == -1 || revision(value);
    }
}
