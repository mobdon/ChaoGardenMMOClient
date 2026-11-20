package garden.tiny.widget;

import android.content.Context;
import android.os.SystemClock;
import org.json.JSONObject;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Native adapter to the Rust accounts-v2 HTTP contract. No gameplay effects live here. */
final class GardenRepository {
    static final ExecutorService IO=Executors.newSingleThreadExecutor();
    static final String PROCESS=UUID.randomUUID().toString();
    private final Context context;
    private final SecureStore store;
    GardenRepository(Context context) { this.context=context.getApplicationContext(); store=new SecureStore(context); }
    private Gateway gateway(JSONObject state) throws Exception { return new Gateway(state.getString("origin"),state.getString("token")); }
    void refresh() throws Exception {
        JSONObject state=store.read();
        if(!state.has("token")) return;
        try { accept(state,AccountAdapter.parse(gateway(state).account())); }
        catch(Exception failure) { fail(state,failure); throw failure; }
    }
    void care(String action,String id,long revision,long snapshotElapsed,long clickedElapsed,java.util.function.BooleanSupplier active) throws Exception {
        JSONObject state=store.read();
        if(!state.has("token") || !state.has("snapshot")) return;
        long gestureStarted=clickedElapsed;
        try {
            if(!java.util.Arrays.asList("pet","rest","wake").contains(action)) throw new IllegalStateException("Open Tiny Garden for this care action.");
            if(state.has("pendingV2")) throw new IllegalStateException("Care confirmation is pending. Refresh to reconcile; it will not be replayed.");
            Snapshot shown=new Snapshot(state.getJSONObject("snapshot"));
            if(!shown.id.equals(id) || shown.revision!=revision || snapshotElapsed!=state.optLong("receivedElapsed",-1)
                    || SecureStore.age(state)>120000 || state.has("error")) throw new IllegalStateException("Refresh your garden before caring.");
            Gateway api=gateway(state);
            AccountAdapter.AccountView account=AccountAdapter.parse(api.account());
            if(account.revision!=revision || account.watch==null || !id.equals(account.chaoId)) {
                accept(state,account); throw new IllegalStateException("Garden changed. Check the refreshed snapshot before caring.");
            }
            JSONObject oldWatch=state.optJSONObject("watch");
            if(oldWatch==null || !oldWatch.getString("id").equals(account.watch.getString("id")))
                throw new IllegalStateException("Garden activity changed. Refresh before caring.");
            JSONObject lease=account.lease;
            boolean acquired=false;
            long currentRevision=account.revision;
            if(lease!=null && account.deviceId.equals(lease.optString("deviceId")) && !"watch".equals(lease.optString("activity")))
                throw new IllegalStateException("Finish the active activity in Tiny Garden before caring.");
            // Client wall time is only a hint about trying acquire. Rust validates expiry and exclusivity.
            if(lease==null || !account.deviceId.equals(lease.optString("deviceId")) || lease.optLong("expiresAt")<=System.currentTimeMillis()) {
                requireRecentGesture(gestureStarted,active);
                JSONObject response=api.accountRequest("lease",new JSONObject().put("operation","acquire").put("activity","watch").put("expectedRevision",currentRevision));
                JSONObject identity=response.getJSONObject("identity"); validateIdentity(state,identity);
                currentRevision=Snapshot.integer(identity,"revision",currentRevision,Snapshot.MAX_SAFE);
                lease=identity.getJSONObject("lease"); acquired=true;
            }
            if(!account.deviceId.equals(lease.getString("deviceId")) || !"watch".equals(lease.getString("activity"))) throw new IllegalStateException("This device does not own watch care.");
            long sequence=Snapshot.integer(lease,"lastSequence",0,Snapshot.MAX_SAFE-1)+1;
            JSONObject body=new JSONObject().put("leaseId",Snapshot.text(lease,"id",128)).put("expectedRevision",currentRevision).put("sequence",sequence)
                    .put("message",new JSONObject().put("type","action").put("action","watch-care").put("sessionId",account.watch.getString("id")).put("kind",action));
            requireRecentGesture(gestureStarted,active);
            state.put("pendingV2",body); store.write(state);
            JSONObject ack=api.accountRequest("intent",body);
            JSONObject identity=ack.getJSONObject("identity"); validateIdentity(state,identity);
            if(!"event".equals(ack.getString("type")) || !"watch-saved".equals(ack.getString("kind"))
                    || Snapshot.integer(ack,"sequence",sequence,sequence)!=sequence || !(ack.get("accepted") instanceof Boolean))
                throw new IllegalStateException("Care confirmation is incomplete. Refresh to reconcile.");
            JSONObject acknowledgedLease=identity.getJSONObject("lease");
            if(!lease.getString("id").equals(acknowledgedLease.getString("id")) || Snapshot.integer(acknowledgedLease,"lastSequence",sequence,Snapshot.MAX_SAFE)<sequence)
                throw new IllegalStateException("Care confirmation does not match its sequence.");
            long acknowledgedRevision=Snapshot.integer(identity,"revision",currentRevision,Snapshot.MAX_SAFE);
            state.remove("pendingV2"); store.write(state);
            // Release only a lease acquired for this tap. Background reads never seize or renew a lease.
            if(acquired) try {
                api.accountRequest("lease",new JSONObject().put("operation","release").put("leaseId",lease.getString("id")).put("expectedRevision",acknowledgedRevision));
            } catch(Exception releaseUnavailable) { /* Rust expires this lease; the next read reconciles. */ }
            accept(state,AccountAdapter.parse(api.account()));
        } catch(Exception failure) { fail(state,failure); throw failure; }
    }
    private static void requireRecentGesture(long started,java.util.function.BooleanSupplier active) {
        long now=SystemClock.elapsedRealtime();
        if(!active.getAsBoolean() || started<0 || now<started || now-started>15000) throw new IllegalStateException("Care tap expired. Refresh and tap again.");
    }
    private static void validateIdentity(JSONObject state,JSONObject identity) throws Exception {
        Snapshot.integer(identity,"schemaVersion",2,2);
        if(!state.getString("chaoId").equals(identity.getString("chaoId")) || !state.getString("accountId").equals(identity.getString("accountId"))
                || !state.getString("deviceId").equals(identity.getString("deviceId"))) throw new IllegalStateException("Account identity changed.");
    }
    private void accept(JSONObject state,AccountAdapter.AccountView account) throws Exception {
        validateIdentity(state,account.identity);
        JSONObject previous=state.optJSONObject("snapshot");
        if(!CarePolicy.acceptsSnapshot(state.getString("chaoId"),state.optLong("pairedRevision",0),previous==null?-1:previous.getLong("revision"),account.chaoId,account.revision))
            throw new IllegalStateException("Snapshot identity or revision does not match this paired Chao.");
        JSONObject art=OriginalCatalog.forPlayer(account.player);
        if(art!=null) account.projection.getJSONObject("chao").put("appearance",art);
        Snapshot next=new Snapshot(account.projection);
        JSONObject pending=state.optJSONObject("pendingV2");
        if(pending!=null && CarePolicy.pendingIsFenced(pending.getLong("expectedRevision"),pending.getString("leaseId"),pending.getLong("sequence"),
                account.revision,account.lease==null?null:account.lease.getString("id"),account.lastSequence)) state.remove("pendingV2");
        state.put("snapshot",next.json).put("identity",account.identity).put("watch",account.watch==null?JSONObject.NULL:account.watch)
                .put("httpCare",Boolean.TRUE.equals(account.capabilities.opt("httpIntents")) && Boolean.TRUE.equals(account.capabilities.opt("activityLeases")) && Boolean.TRUE.equals(account.capabilities.opt("intentSequence")))
                .put("receivedElapsed",SystemClock.elapsedRealtime()).put("receivedWall",System.currentTimeMillis()).put("process",PROCESS);
        state.remove("error"); store.write(state);
        try { OriginalArtwork.update(context,gateway(state),next); } catch(Exception unavailable) { /* Explicit missing-original marker remains. */ }
    }
    private void fail(JSONObject state,Exception failure) throws Exception {
        if(failure instanceof Gateway.Failure && (((Gateway.Failure)failure).status==401 || ((Gateway.Failure)failure).status==403)) {
            store.clear(); OriginalArtwork.clear(context); return;
        }
        state.put("error",failure instanceof Gateway.Failure || failure instanceof IllegalStateException?failure.getMessage():"Garden unavailable. Cached data only.");
        state.remove("process"); store.write(state);
    }
}
