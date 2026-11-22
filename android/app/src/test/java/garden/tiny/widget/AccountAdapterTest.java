package garden.tiny.widget;

import static org.junit.Assert.*;
import org.json.JSONException;
import org.json.JSONObject;
import org.junit.Test;

/** Synthetic POST /api/session replies; does not claim a live service. */
public final class AccountAdapterTest {
    private static JSONObject reply() throws JSONException {
        JSONObject stats = new JSONObject();
        for (int i = 0; i < Snapshot.SKILLS.length; i++) {
            stats.put(Snapshot.SKILLS[i], new JSONObject().put("level", 10 + i)
                    .put("grade", i < 5 ? i : 200 + i).put("points", 1000 + i));
        }
        return new JSONObject().put("token", "synthetic-token").put("player", new JSONObject()
                .put("id", "chao-synthetic").put("name", "Tester")
                .put("form", "chaos").put("alignment", -100).put("rings", 123)
                .put("energy", 84.0).put("hunger", 72.0).put("happiness", 96)
                .put("stats", stats).put("zone", "dark")
                .put("watch", new JSONObject().put("id", "watch-synthetic").put("activity", "wander")));
    }

    @Test public void readsTheLoggedInChaoFromASessionReply() throws Exception {
        AccountAdapter.AccountView view = AccountAdapter.parse(reply());
        assertEquals("chao-synthetic", view.chaoId);
        Snapshot snapshot = view.snapshot();
        assertEquals("Tester", snapshot.name);
        assertEquals(72.0, snapshot.needs.getDouble("fullness"), 0);
        assertEquals("Wandering in Dark Garden", snapshot.activity);
        assertEquals("unavailable", snapshot.training.getString("status"));
        assertEquals(0, snapshot.care.getJSONArray("actions").length());
        assertEquals(16, snapshot.stats.getJSONObject("intelligence").getInt("level"));
    }

    @Test public void aChaoThatIsNotOutInAGardenHasNoWatch() throws Exception {
        JSONObject reply = reply(); reply.getJSONObject("player").put("watch", JSONObject.NULL);
        AccountAdapter.AccountView view = AccountAdapter.parse(reply);
        assertNull(view.watch);
        assertEquals("In Dark Garden", view.snapshot().activity);
    }

    @Test public void laterChangesToTheReplyDoNotReachTheView() throws Exception {
        JSONObject reply = reply();
        AccountAdapter.AccountView view = AccountAdapter.parse(reply);
        reply.getJSONObject("player").put("name", "Changed");
        assertEquals("Tester", view.snapshot().name);
    }

    @Test public void malformedRepliesAreRefused() throws Exception {
        JSONObject noPlayer = reply(); noPlayer.remove("player");
        JSONObject noId = reply(); noId.getJSONObject("player").remove("id");
        JSONObject badNeed = reply(); badNeed.getJSONObject("player").put("hunger", 140);
        JSONObject badWatch = reply(); badWatch.getJSONObject("player").getJSONObject("watch").put("activity", "flying");
        for (JSONObject bad : new JSONObject[]{noPlayer, noId, badNeed, badWatch}) {
            try { AccountAdapter.parse(bad); fail("Malformed session reply accepted"); }
            catch (JSONException expected) { }
        }
    }
}
