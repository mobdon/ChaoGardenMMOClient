package garden.tiny.widget;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Read-only adapter for the player in a POST /api/session reply. */
public final class AccountAdapter {
    private AccountAdapter() { }

    public static final class AccountView {
        public final JSONObject player, watch, projection;
        public final String chaoId;
        private final Snapshot snapshot;

        private AccountView(JSONObject player, JSONObject watch, JSONObject projection) throws JSONException {
            this.player = player;
            this.watch = watch;
            this.projection = projection;
            chaoId = Snapshot.text(player, "id", 128);
            snapshot = new Snapshot(projection);
        }

        public Snapshot snapshot() { return snapshot; }
    }

    public static AccountView parse(JSONObject reply) throws JSONException {
        // Work on a detached response so later callers cannot change validated input.
        JSONObject player = new JSONObject(reply.getJSONObject("player").toString());
        String chaoId = Snapshot.text(player, "id", 128);
        JSONObject watch = nullableObject(player, "watch");
        JSONObject activity = new JSONObject();
        String garden = zoneLabel(Snapshot.text(player, "zone", 64));
        if (watch != null) {
            String watchId = Snapshot.text(watch, "id", 128);
            String phase = Snapshot.text(watch, "activity", 32);
            String label;
            switch (phase) {
                case "idle": label = "Idle"; break;
                case "wander": label = "Wandering"; break;
                case "rest": label = "Resting"; break;
                default: throw new JSONException("Unsupported resident activity");
            }
            activity.put("label", label + " in " + garden).put("watchId", watchId);
        } else {
            activity.put("label", "In " + garden);
        }

        JSONObject chao = new JSONObject()
                .put("id", chaoId)
                .put("name", Snapshot.text(player, "name", 64))
                .put("form", Snapshot.text(player, "form", 64))
                .put("alignment", Snapshot.number(player, "alignment", -100, 100))
                .put("rings", Snapshot.integer(player, "rings", 0, 999999))
                .put("needs", new JSONObject()
                        .put("energy", Snapshot.number(player, "energy", 0, 100))
                        .put("fullness", Snapshot.number(player, "hunger", 0, 100))
                        .put("happiness", Snapshot.number(player, "happiness", 0, 100)))
                .put("stats", new JSONObject(player.getJSONObject("stats").toString()));

        // The session reply carries no server clock, save time or training job: each stays explicitly unavailable.
        JSONObject projection = new JSONObject()
                .put("protocol", Snapshot.PROTOCOL)
                .put("revision", 0)
                .put("serverTimeMs", JSONObject.NULL)
                .put("lastSavedAtMs", JSONObject.NULL)
                .put("chao", chao)
                .put("activity", activity)
                .put("training", new JSONObject().put("status", "unavailable"))
                .put("care", new JSONObject().put("actions", new JSONArray()));
        return new AccountView(player, watch, projection);
    }

    private static JSONObject nullableObject(JSONObject value, String key) throws JSONException {
        if (!value.has(key)) throw new JSONException("Missing account field: " + key);
        return value.isNull(key) ? null : value.getJSONObject(key);
    }

    private static String zoneLabel(String zone) {
        switch (zone) {
            case "garden": return "Neutral Garden";
            case "hero": return "Hero Garden";
            case "dark": return "Dark Garden";
            case "hub": return "the hub";
            case "mission": return "a mission";
            case "race": return "a race";
            case "karate": return "Karate";
            default: return zone;
        }
    }
}
