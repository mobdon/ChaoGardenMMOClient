package garden.tiny.widget;

import android.content.Context;
import android.os.SystemClock;
import org.json.JSONObject;
import java.util.UUID;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Reads the logged-in Chao from the server. No gameplay effects live here. */
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
        try {
            Gateway api=gateway(state);
            byte[] own=api.ownChao(); // refused until the same account has played once on its PC
            accept(state,AccountAdapter.parse(api.session()),api);
            OriginalArtwork.saveOwn(context,own);
        }
        catch(Exception failure) { fail(state,failure); throw failure; }
    }
    /** One care tap from the widget; the server applies it and answers with the Chao as it is now. */
    void care(String kind) throws Exception {
        JSONObject state=store.read();
        if(!state.has("token")) return;
        try { Gateway api=gateway(state); accept(state,AccountAdapter.parse(api.care(kind)),api); }
        catch(Exception failure) { fail(state,failure); throw failure; }
    }
    private void accept(JSONObject state,AccountAdapter.AccountView account,Gateway api) throws Exception {
        if(!state.getString("chaoId").equals(account.chaoId)) throw new IllegalStateException("This login now opens a different Chao. Log in again.");
        JSONObject art=OriginalCatalog.forPlayer(account.player);
        if(art!=null) account.projection.getJSONObject("chao").put("appearance",art);
        Snapshot next=new Snapshot(account.projection);
        state.put("snapshot",next.json).put("watch",account.watch==null?JSONObject.NULL:account.watch)
                .put("receivedElapsed",SystemClock.elapsedRealtime()).put("receivedWall",System.currentTimeMillis()).put("process",PROCESS);
        state.remove("error"); store.write(state);
        try { OriginalArtwork.update(context,api,next); } catch(Exception unavailable) { /* Explicit missing-original marker remains. */ }
    }
    private void fail(JSONObject state,Exception failure) throws Exception {
        if(failure instanceof Gateway.Failure && (((Gateway.Failure)failure).status==401 || ((Gateway.Failure)failure).status==403)) {
            store.clear(); OriginalArtwork.clear(context); return;
        }
        state.put("error",failure instanceof Gateway.Failure || failure instanceof IllegalStateException?failure.getMessage():"Garden unavailable. Cached data only.");
        state.remove("process"); store.write(state);
    }
}
