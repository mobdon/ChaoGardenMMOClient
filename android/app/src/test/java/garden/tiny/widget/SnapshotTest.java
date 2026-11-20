package garden.tiny.widget;

import static org.junit.Assert.*;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import org.junit.Test;

public final class SnapshotTest {
    private static final long SERVER_TIME = 1_700_000_000_000L;

    private static JSONObject valid() throws JSONException {
        JSONObject stats = new JSONObject();
        for (int i = 0; i < Snapshot.SKILLS.length; i++) {
            stats.put(Snapshot.SKILLS[i], new JSONObject().put("level", 42 + i)
                    .put("grade", i < 5 ? 5 : 255).put("points", 2100 + i));
        }
        return new JSONObject().put("protocol", Snapshot.PROTOCOL).put("revision", 12)
                .put("serverTimeMs", SERVER_TIME).put("lastSavedAtMs", SERVER_TIME - 1000)
                .put("chao", new JSONObject().put("id", "test-chao").put("name", "Test fixture")
                        .put("form", "dark-chaos").put("rings", 17)
                        .put("needs", new JSONObject().put("energy", 70.5).put("fullness", 55).put("happiness", 82))
                        .put("stats", stats))
                .put("activity", new JSONObject().put("label", "Resting in Dark Garden"))
                .put("training", new JSONObject().put("status", "unavailable"))
                .put("care", new JSONObject().put("actions", new JSONArray().put("pet").put("feed"))
                        .put("capability", "test-only-server-capability").put("expiresAtMs", SERVER_TIME + 120_000));
    }

    private static void rejected(JSONObject value) throws Exception {
        try { new Snapshot(value); fail("Accepted a malformed authoritative snapshot"); }
        catch (JSONException expected) { /* no invented fallback progression */ }
    }

    @Test public void projectsAllSevenStatsAndKeepsHiddenNumericGrades() throws Exception {
        Snapshot snapshot = new Snapshot(valid());
        assertEquals(12, snapshot.revision);
        assertEquals("test-chao", snapshot.id);
        for (String skill : Snapshot.SKILLS) assertTrue(snapshot.statsText(true).contains(skill));
        assertTrue(snapshot.statsText(true).contains("Grade S"));
        assertTrue(snapshot.statsText(true).contains("Grade 255"));
        assertTrue(snapshot.statsText(true).contains("2106 pts"));
        assertTrue(snapshot.needsText().contains("Fullness 55%"));
        assertFalse(snapshot.needsText().contains("Hunger"));
    }

    @Test public void rejectsMissingStatsAndWrongGradeOrLevelRanges() throws Exception {
        JSONObject missing = valid(); missing.getJSONObject("chao").getJSONObject("stats").remove("intelligence"); rejected(missing);
        for (String skill : Snapshot.SKILLS) {
            JSONObject level = valid(); level.getJSONObject("chao").getJSONObject("stats").getJSONObject(skill).put("level", 100); rejected(level);
            JSONObject grade = valid(); grade.getJSONObject("chao").getJSONObject("stats").getJSONObject(skill).put("grade", skill.equals("luck") || skill.equals("intelligence") ? 256 : 6); rejected(grade);
            JSONObject points = valid(); points.getJSONObject("chao").getJSONObject("stats").getJSONObject(skill).put("points", -1); rejected(points);
        }
    }

    @Test public void rejectsNumericStringsInsteadOfCoercingThem() throws Exception {
        JSONObject revision = valid().put("revision", "12"); rejected(revision);
        JSONObject needs = valid(); needs.getJSONObject("chao").getJSONObject("needs").put("energy", "99"); rejected(needs);
        JSONObject grade = valid(); grade.getJSONObject("chao").getJSONObject("stats").getJSONObject("luck").put("grade", "255"); rejected(grade);
        JSONObject level = valid(); level.getJSONObject("chao").getJSONObject("stats").getJSONObject("run").put("level", "42"); rejected(level);
        for(String key:new String[]{"id","name","form"}) {
            JSONObject identity=valid(); identity.getJSONObject("chao").put(key,42); rejected(identity);
        }
    }

    @Test public void rejectsFractionalIntegersAfterJsonWireDecoding() throws Exception {
        JSONObject revision = valid();
        String revisionWire = revision.toString().replace("\"revision\":12", "\"revision\":12.75");
        rejected(new JSONObject(revisionWire));
        JSONObject grade = valid(); grade.getJSONObject("chao").getJSONObject("stats").getJSONObject("run").put("grade", new java.math.BigDecimal("4.5"));
        rejected(new JSONObject(grade.toString()));
        JSONObject points = valid(); points.getJSONObject("chao").getJSONObject("stats").getJSONObject("swim").put("points", new java.math.BigDecimal("2100.25"));
        rejected(new JSONObject(points.toString()));
    }

    @Test public void rejectsNeedsOutsideRangeAndFutureSavedTime() throws Exception {
        for (double value : new double[]{-0.1, 100.1}) {
            JSONObject data = valid(); data.getJSONObject("chao").getJSONObject("needs").put("fullness", value); rejected(data);
        }
        rejected(valid().put("lastSavedAtMs", SERVER_TIME + 1));
        rejected(valid().put("revision", -1));
        rejected(valid().put("revision", 9_007_199_254_740_992L));
        rejected(valid().put("serverTimeMs", 9_007_199_254_740_992L));
        JSONObject tooManyPoints=valid(); tooManyPoints.getJSONObject("chao").getJSONObject("stats").getJSONObject("run").put("points",10000); rejected(tooManyPoints);
    }

    @Test public void explicitUnknownSaveTimeDoesNotInventAcknowledgement() throws Exception {
        assertEquals(-1,new Snapshot(valid().put("lastSavedAtMs",JSONObject.NULL)).savedAtMs);
        JSONObject missing=valid(); missing.remove("lastSavedAtMs"); rejected(missing);
    }

    @Test public void requiresExplicitTrainingAvailabilityAndServerPhase() throws Exception {
        assertEquals("AFK training unavailable", new Snapshot(valid()).trainingText());
        JSONObject missing = valid(); missing.remove("training"); rejected(missing);
        JSONObject unknown = valid(); unknown.getJSONObject("training").put("status", "complete-ish"); rejected(unknown);
        JSONObject none = valid().put("training", new JSONObject().put("status", "none"));
        assertTrue(new Snapshot(none).trainingText().contains("No AFK job"));
    }

    @Test public void pastEndDoesNotInventSettlementOrChangeServerPhase() throws Exception {
        JSONObject data = valid().put("training", new JSONObject().put("status", "training")
                .put("skill", "run").put("endAtMs", SERVER_TIME - 60_000));
        Snapshot snapshot = new Snapshot(data);
        assertTrue(snapshot.trainingText().contains("training"));
        assertFalse(snapshot.trainingText().contains("settled"));
        assertEquals("training", snapshot.json.getJSONObject("training").getString("status"));
        assertEquals(44, snapshot.stats.getJSONObject("run").getInt("level"));
    }

    @Test public void trainingCannotClaimUnsupportedHiddenSkill() throws Exception {
        rejected(valid().put("training", new JSONObject().put("status", "ready").put("skill", "intelligence").put("endAtMs", SERVER_TIME)));
    }

    @Test public void careNeedsPermissionFreshnessAndUnexpiredCapability() throws Exception {
        Snapshot snapshot = new Snapshot(valid());
        assertTrue(snapshot.allows("pet", 0));
        assertTrue(snapshot.allows("feed", 119_999));
        assertFalse(snapshot.allows("feed", 120_000));
        assertFalse(snapshot.allows("pet", -1));
        assertFalse(snapshot.allows("pet", 120_001));
        assertFalse(snapshot.allows("train", 0));
        JSONObject viewOnly = valid().put("care", new JSONObject().put("actions", new JSONArray()));
        assertFalse(new Snapshot(viewOnly).allows("pet", 0));
    }

    @Test public void rejectsMissingOrAlreadyExpiredCareAuthority() throws Exception {
        JSONObject missing = valid(); missing.getJSONObject("care").remove("capability"); rejected(missing);
        JSONObject expired = valid(); expired.getJSONObject("care").put("expiresAtMs", SERVER_TIME); rejected(expired);
        JSONObject action = valid(); action.getJSONObject("care").getJSONArray("actions").put("award-rings"); rejected(action);
    }
}
