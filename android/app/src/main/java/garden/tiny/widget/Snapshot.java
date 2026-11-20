package garden.tiny.widget;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import java.util.Arrays;

/** Display-only projection. No progression, care effects or training settlement exists here. */
public final class Snapshot {
    public static final String PROTOCOL = "tinygarden.widget.v1";
    public static final long MAX_SAFE = 9007199254740991L;
    public static final String[] SKILLS = {"swim", "fly", "run", "power", "stamina", "luck", "intelligence"};
    public final JSONObject json, chao, needs, stats, training, care, appearance;
    public final String id, name, form, activity;
    public final long revision, serverTimeMs, savedAtMs;
    public Snapshot(JSONObject data) throws JSONException {
        if (!PROTOCOL.equals(data.getString("protocol"))) throw new JSONException("Unsupported widget protocol");
        json = data; chao = data.getJSONObject("chao");
        id = text(chao,"id",128); name = text(chao,"name",64); form = text(chao,"form",64);
        revision = integer(data, "revision", 0, 9007199254740991L);
        if(!data.has("serverTimeMs")) throw new JSONException("Missing server time availability");
        serverTimeMs = data.isNull("serverTimeMs") ? -1 : integer(data, "serverTimeMs", 1, MAX_SAFE);
        if(!data.has("lastSavedAtMs")) throw new JSONException("Missing save confirmation");
        savedAtMs = data.isNull("lastSavedAtMs") ? -1 : integer(data, "lastSavedAtMs", 1, serverTimeMs);
        integer(chao, "rings", 0, 9007199254740991L);
        needs = chao.getJSONObject("needs");
        for (String key : new String[]{"energy", "fullness", "happiness"}) number(needs, key, 0, 100);
        stats = chao.getJSONObject("stats");
        for (int i = 0; i < SKILLS.length; i++) {
            JSONObject stat = stats.getJSONObject(SKILLS[i]);
            integer(stat, "level", 0, 99); integer(stat, "grade", 0, i < 5 ? 5 : 255); integer(stat, "points", 0, 9999);
        }
        activity = text(data.getJSONObject("activity"),"label",100);
        training = data.getJSONObject("training");
        String status = training.getString("status");
        if (!Arrays.asList("unavailable", "none", "training", "ready", "settled", "canceled").contains(status)) throw new JSONException("Unknown training phase");
        if (!Arrays.asList("unavailable", "none").contains(status)) {
            if (!Arrays.asList("swim", "fly", "run", "power", "stamina").contains(training.getString("skill"))) throw new JSONException("Invalid training skill");
            integer(training, "endAtMs", 1, MAX_SAFE);
        }
        care = data.getJSONObject("care");
        JSONArray actions = care.getJSONArray("actions");
        for (int i=0; i<actions.length(); i++) if (!Arrays.asList("pet", "feed").contains(actions.getString(i))) throw new JSONException("Unknown care action");
        if (actions.length() > 0) {
            if(serverTimeMs<0) throw new JSONException("Care capability requires authoritative time");
            bounded(care.getString("capability"), 2048); integer(care, "expiresAtMs", serverTimeMs + 1, MAX_SAFE);
        }
        appearance = chao.optJSONObject("appearance");
    }
    static String bounded(String value, int max) throws JSONException {
        if (value.trim().isEmpty() || value.length() > max || value.chars().anyMatch(c -> Character.isISOControl(c))) throw new JSONException("Invalid text");
        return value;
    }
    static String text(JSONObject object,String key,int max) throws JSONException {
        Object raw=object.get(key);
        if(!(raw instanceof String)) throw new JSONException("Expected text: "+key);
        return bounded((String)raw,max);
    }
    static long integer(JSONObject value, String key, long min, long max) throws JSONException {
        Object raw = value.get(key);
        if (!(raw instanceof Integer) && !(raw instanceof Long)) throw new JSONException("Expected integer: " + key);
        long n = ((Number)raw).longValue();
        if (n < min || n > max) throw new JSONException("Out of range: " + key);
        return n;
    }
    static double number(JSONObject value, String key, double min, double max) throws JSONException {
        Object raw = value.get(key);
        if (!(raw instanceof Number)) throw new JSONException("Expected number: " + key);
        double n = ((Number)raw).doubleValue();
        if (!Double.isFinite(n) || n < min || n > max) throw new JSONException("Out of range: " + key);
        return n;
    }
    public boolean allows(String action, long ageMs) {
        if (ageMs < 0 || ageMs > 120000) return false;
        JSONArray actions = care.optJSONArray("actions");
        for (int i=0; i<actions.length(); i++) if (action.equals(actions.optString(i)))
            return serverTimeMs + ageMs < care.optLong("expiresAtMs");
        return false;
    }
    public String needsText() {
        return "Energy " + Math.round(needs.optDouble("energy")) + "% · Fullness " + Math.round(needs.optDouble("fullness")) + "% · Happy " + Math.round(needs.optDouble("happiness")) + "%";
    }
    public String statsText(boolean large) {
        StringBuilder out = new StringBuilder();
        for (int i=0;i<SKILLS.length;i++) {
            JSONObject stat = stats.optJSONObject(SKILLS[i]);
            if (i>0) out.append(large ? "\n" : " · ");
            out.append(large ? SKILLS[i] : new String[]{"Sw","Fl","Ru","Pw","St","Lu","IQ"}[i]).append(" ").append(stat.optInt("level"));
            if (large) out.append("  Grade ").append(i < 5 ? new String[]{"E","D","C","B","A","S"}[stat.optInt("grade")] : stat.optInt("grade")).append("  · ").append(stat.optLong("points")).append(" pts");
        }
        return out.toString();
    }
    public String trainingText() {
        String status = training.optString("status");
        if ("unavailable".equals(status)) return "AFK training unavailable";
        if ("none".equals(status)) return "No AFK job · train in app";
        String end = new java.text.SimpleDateFormat("MMM d HH:mm", java.util.Locale.getDefault()).format(new java.util.Date(training.optLong("endAtMs")));
        return training.optString("skill") + " · " + status + " · end " + end;
    }
}
