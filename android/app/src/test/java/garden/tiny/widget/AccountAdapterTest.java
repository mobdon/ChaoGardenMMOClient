package garden.tiny.widget;

import static org.junit.Assert.*;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import org.junit.Test;

/** Synthetic exact accounts-v2 envelopes; does not claim a live paired service. */
public final class AccountAdapterTest {
    private static JSONObject identity() throws JSONException {
        return new JSONObject().put("schemaVersion", 2).put("accountId", "account-original")
                .put("chaoId", "chao-original").put("deviceId", "android-original")
                .put("revision", 42).put("devices", new JSONArray())
                .put("lease", JSONObject.NULL);
    }

    private static JSONObject player(String id) throws JSONException {
        JSONObject stats = new JSONObject();
        for (int i = 0; i < Snapshot.SKILLS.length; i++) {
            stats.put(Snapshot.SKILLS[i], new JSONObject().put("level", 10 + i)
                    .put("grade", i < 5 ? i : 200 + i).put("points", 1000 + i));
        }
        return new JSONObject().put("id", id).put("name", "Actual Chao")
                .put("form", "chaos").put("alignment", -100).put("rings", 123)
                .put("energy", 84).put("hunger", 72).put("happiness", 96)
                .put("stats", stats).put("zone", "dark")
                .put("watch", new JSONObject().put("id", "watch-original").put("activity", "wander"));
    }

    private static JSONObject valid() throws JSONException {
        JSONObject identity = identity();
        return new JSONObject().put("identity", identity)
                .put("capabilities", new JSONObject().put("version", 2)
                        .put("durableTraining", false).put("httpIntents", true))
                .put("snapshot", new JSONObject().put("type", "snapshot").put("protocolVersion", 2)
                        .put("selfId", "chao-original").put("serverTime", 1250.0)
                        .put("identity", new JSONObject(identity.toString()))
                        .put("players", new JSONArray().put(player("other-player")).put(player("chao-original"))));
    }

    private static void rejected(JSONObject account) throws Exception {
        try { AccountAdapter.parse(account); fail("Malformed accounts snapshot accepted"); }
        catch (JSONException expected) { }
    }

    @Test public void selectsCanonicalSelfRatherThanFirstPublicPlayer() throws Exception {
        AccountAdapter.AccountView view = AccountAdapter.parse(valid());
        assertEquals("chao-original", view.chaoId);
        assertEquals("account-original", view.accountId);
        assertEquals("android-original", view.deviceId);
        assertEquals(42, view.revision);
        assertEquals("chao-original", view.snapshot().id);
        assertEquals("Wandering in Dark Garden", view.snapshot().activity);
        assertEquals("watch-original", view.watch.getString("id"));
        assertNull(view.lease);
        assertEquals(-1, view.lastSequence);
    }

    @Test public void projectsActualNeedsFullnessAndAllSevenStatValues() throws Exception {
        AccountAdapter.AccountView view = AccountAdapter.parse(valid());
        assertEquals(84.0, view.snapshot().needs.getDouble("energy"), 0);
        assertEquals(72.0, view.snapshot().needs.getDouble("fullness"), 0);
        assertEquals(96.0, view.snapshot().needs.getDouble("happiness"), 0);
        for (int i = 0; i < Snapshot.SKILLS.length; i++) {
            JSONObject stat = view.snapshot().stats.getJSONObject(Snapshot.SKILLS[i]);
            assertEquals(10 + i, stat.getInt("level"));
            assertEquals(i < 5 ? i : 200 + i, stat.getInt("grade"));
            assertEquals(1000 + i, stat.getInt("points"));
        }
        assertEquals(-100.0, view.snapshot().chao.getDouble("alignment"), 0);
        assertEquals(123, view.snapshot().chao.getInt("rings"));
    }

    @Test public void simulationClockNeverBecomesServerUtcOrSaveConfirmation() throws Exception {
        AccountAdapter.AccountView view = AccountAdapter.parse(valid());
        assertEquals(1250.0, view.simulationTimeMs, 0);
        assertTrue(view.projection.isNull("serverTimeMs"));
        assertTrue(view.projection.isNull("lastSavedAtMs"));
        assertEquals(-1, view.snapshot().serverTimeMs);
        assertEquals(-1, view.snapshot().savedAtMs);
    }

    @Test public void trainingCareAndUnhashedArtRemainExplicitlyUnavailable() throws Exception {
        JSONObject account = valid();
        account.getJSONObject("capabilities").put("durableTraining", true);
        AccountAdapter.AccountView view = AccountAdapter.parse(account);
        // A true capability without a supported job schema still cannot invent a job.
        assertEquals("unavailable", view.snapshot().training.getString("status"));
        assertEquals(0, view.snapshot().care.getJSONArray("actions").length());
        assertFalse(view.snapshot().allows("pet", 0));
        assertFalse(view.snapshot().allows("feed", 0));
        assertNull(view.snapshot().appearance);
    }

    @Test public void exposesExactLeaseCounterForFutureFencedIntentIntegration() throws Exception {
        JSONObject account = valid();
        JSONObject lease = new JSONObject().put("id", "lease-original")
                .put("deviceId", "android-original").put("activity", "watch")
                .put("expiresAt", 1795000030000L).put("lastSequence", 7);
        account.getJSONObject("identity").put("lease", lease);
        account.getJSONObject("snapshot").getJSONObject("identity").put("lease", new JSONObject(lease.toString()));
        AccountAdapter.AccountView view = AccountAdapter.parse(account);
        assertEquals("lease-original", view.lease.getString("id"));
        assertEquals(7, view.lastSequence);
        assertEquals(1795000030000L, view.lease.getLong("expiresAt"));
        assertEquals(-1, view.snapshot().serverTimeMs);
    }

    @Test public void offlineNonresidentMissingSelfFailsWithoutSubstitution() throws Exception {
        JSONObject account = valid();
        account.getJSONObject("snapshot").put("players", new JSONArray().put(player("other-player")));
        rejected(account);
        JSONObject duplicate = valid();
        duplicate.getJSONObject("snapshot").getJSONArray("players").put(player("chao-original"));
        rejected(duplicate);
    }

    @Test public void canonicalIdentityAndPrivateEnvelopeMustAgree() throws Exception {
        JSONObject wrongSelf = valid(); wrongSelf.getJSONObject("snapshot").put("selfId", "other-player"); rejected(wrongSelf);
        for (String field : new String[]{"accountId", "chaoId", "deviceId"}) {
            JSONObject mismatch = valid();
            mismatch.getJSONObject("snapshot").getJSONObject("identity").put(field, "other");
            rejected(mismatch);
        }
        JSONObject revision = valid(); revision.getJSONObject("snapshot").getJSONObject("identity").put("revision", 41); rejected(revision);
    }

    @Test public void rejectsCoercedVersionsIncompleteNeedsAndMalformedLeaseCounters() throws Exception {
        JSONObject version = valid(); version.getJSONObject("capabilities").put("version", "2"); rejected(version);
        JSONObject schema = valid(); schema.getJSONObject("identity").put("schemaVersion", 2.5); rejected(schema);
        JSONObject protocol = valid(); protocol.getJSONObject("snapshot").put("protocolVersion", 1); rejected(protocol);
        JSONObject training = valid(); training.getJSONObject("capabilities").put("durableTraining", "false"); rejected(training);
        JSONObject needs = valid(); needs.getJSONObject("snapshot").getJSONArray("players").getJSONObject(1).remove("hunger"); rejected(needs);
        JSONObject points = valid(); points.getJSONObject("snapshot").getJSONArray("players").getJSONObject(1)
                .getJSONObject("stats").getJSONObject("run").put("points", 10000); rejected(points);
        JSONObject lease = valid(); lease.getJSONObject("identity").put("lease", new JSONObject()
                .put("id", "lease").put("deviceId", "android-original").put("activity", "watch")
                .put("expiresAt", 1795000030000L).put("lastSequence", "7")); rejected(lease);
    }

    @Test public void nullWatchProjectsOnlyConfirmedGardenLocation() throws Exception {
        JSONObject account = valid();
        account.getJSONObject("snapshot").getJSONArray("players").getJSONObject(1).put("watch", JSONObject.NULL);
        AccountAdapter.AccountView view = AccountAdapter.parse(account);
        assertNull(view.watch);
        assertEquals("In Dark Garden", view.snapshot().activity);
        assertFalse(view.projection.getJSONObject("activity").has("watchId"));
    }
}
