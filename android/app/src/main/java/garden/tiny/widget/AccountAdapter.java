package garden.tiny.widget;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

/** Read-only adapter for the Rust accounts v2 GET /api/account envelope. */
public final class AccountAdapter {
    private AccountAdapter() { }

    public static final class AccountView {
        public final JSONObject identity, capabilities, player, watch, lease, projection;
        public final String accountId, chaoId, deviceId;
        public final long revision, lastSequence;
        /** Server simulation time only. Never convert this to a Date or a save timestamp. */
        public final double simulationTimeMs;
        private final Snapshot snapshot;

        private AccountView(JSONObject identity, JSONObject capabilities, JSONObject player,
                JSONObject watch, JSONObject lease, JSONObject projection, double simulationTimeMs)
                throws JSONException {
            this.identity = identity;
            this.capabilities = capabilities;
            this.player = player;
            this.watch = watch;
            this.lease = lease;
            this.projection = projection;
            accountId = Snapshot.text(identity, "accountId", 128);
            chaoId = Snapshot.text(identity, "chaoId", 128);
            deviceId = Snapshot.text(identity, "deviceId", 128);
            revision = Snapshot.integer(identity, "revision", 0, Snapshot.MAX_SAFE);
            lastSequence = lease == null ? -1 : Snapshot.integer(lease, "lastSequence", 0, Snapshot.MAX_SAFE);
            this.simulationTimeMs = simulationTimeMs;
            snapshot = new Snapshot(projection);
        }

        public Snapshot snapshot() { return snapshot; }
    }

    public static AccountView parse(JSONObject envelope) throws JSONException {
        // Work on a detached response so later callers cannot change validated input.
        JSONObject account = new JSONObject(envelope.toString());
        JSONObject identity = account.getJSONObject("identity");
        identityVersion(identity);
        JSONObject capabilities = account.getJSONObject("capabilities");
        Snapshot.integer(capabilities, "version", 2, 2);
        booleanField(capabilities, "durableTraining");
        JSONObject source = account.getJSONObject("snapshot");
        Snapshot.integer(source, "protocolVersion", 2, 2);
        if (!"snapshot".equals(Snapshot.text(source, "type", 32))) {
            throw new JSONException("Expected canonical account snapshot");
        }
        String chaoId = Snapshot.text(identity, "chaoId", 128);
        if (!chaoId.equals(Snapshot.text(source, "selfId", 128))) {
            throw new JSONException("Account self identity mismatch");
        }
        JSONObject nestedIdentity = source.getJSONObject("identity");
        identityVersion(nestedIdentity);
        for (String field : new String[]{"accountId", "chaoId", "deviceId"}) {
            if (!Snapshot.text(identity, field, 128).equals(Snapshot.text(nestedIdentity, field, 128))) {
                throw new JSONException("Account snapshot identity mismatch");
            }
        }
        long revision = Snapshot.integer(identity, "revision", 0, Snapshot.MAX_SAFE);
        if (revision != Snapshot.integer(nestedIdentity, "revision", 0, Snapshot.MAX_SAFE)) {
            throw new JSONException("Account snapshot revision mismatch");
        }

        JSONArray players = source.getJSONArray("players");
        JSONObject player = null;
        for (int i = 0; i < players.length(); i++) {
            JSONObject candidate = players.getJSONObject(i);
            String id = Snapshot.text(candidate, "id", 128);
            if (chaoId.equals(id)) {
                if (player != null) throw new JSONException("Duplicate canonical Chao in snapshot");
                player = candidate;
            }
        }
        if (player == null) {
            // Incomplete/older services must not substitute a different Chao.
            throw new JSONException("Canonical Chao missing from account snapshot");
        }

        JSONObject lease = nullableObject(identity, "lease");
        if (lease != null) {
            Snapshot.text(lease, "id", 128);
            Snapshot.text(lease, "deviceId", 128);
            String activity = Snapshot.text(lease, "activity", 32);
            if (!java.util.Arrays.asList("direct", "tiny", "watch", "training", "competition").contains(activity)) {
                throw new JSONException("Unsupported account lease activity");
            }
            Snapshot.integer(lease, "expiresAt", 1, Snapshot.MAX_SAFE);
            Snapshot.integer(lease, "lastSequence", 0, Snapshot.MAX_SAFE);
        }
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

        // /api/account does not publish a server UTC read time, confirmed save
        // UTC, AFK job, hashed sprite descriptor or opaque care capability.
        // Keep each unavailable fact explicit rather than reinterpret game.time
        // or infer a capability from possession of a device token/watch ID.
        JSONObject projection = new JSONObject()
                .put("protocol", Snapshot.PROTOCOL)
                .put("revision", revision)
                .put("serverTimeMs", JSONObject.NULL)
                .put("lastSavedAtMs", JSONObject.NULL)
                .put("chao", chao)
                .put("activity", activity)
                .put("training", new JSONObject().put("status", "unavailable"))
                .put("care", new JSONObject().put("actions", new JSONArray()));
        double simulationTimeMs = Snapshot.number(source, "serverTime", 0, Snapshot.MAX_SAFE);
        return new AccountView(identity, capabilities, player, watch, lease, projection, simulationTimeMs);
    }

    private static void identityVersion(JSONObject identity) throws JSONException {
        Snapshot.integer(identity, "schemaVersion", 2, 2);
        Snapshot.text(identity, "accountId", 128);
        Snapshot.text(identity, "chaoId", 128);
        Snapshot.text(identity, "deviceId", 128);
        Snapshot.integer(identity, "revision", 0, Snapshot.MAX_SAFE);
    }

    private static boolean booleanField(JSONObject value, String key) throws JSONException {
        Object raw = value.get(key);
        if (!(raw instanceof Boolean)) throw new JSONException("Expected boolean: " + key);
        return (Boolean) raw;
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
