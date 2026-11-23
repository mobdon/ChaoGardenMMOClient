package garden.tiny.widget;

import static org.junit.Assert.*;
import org.json.JSONObject;
import org.junit.Test;

public final class OriginalCatalogTest {
    private static JSONObject player(String form, int alignment) throws Exception {
        return new JSONObject().put("form", form).put("alignment", alignment);
    }
    private static void variant(String expected, JSONObject player) throws Exception {
        JSONObject art = OriginalCatalog.forPlayer(player);
        assertNotNull(art);
        assertEquals("/api/assets/tiny-" + expected + "-chaos.png", art.getString("atlasPath"));
        assertTrue(art.getString("atlasSha256").matches("[a-f0-9]{64}"));
        assertEquals(96, art.getInt("frameSize"));
        assertEquals(8, art.getInt("columns"));
        assertEquals("Static front idle frame", 0, art.getInt("frameIndex"));
        assertEquals("Only Dark Chaos gets the verified native flame", expected.equals("dark"), art.has("flamePath"));
    }
    @Test public void noNormalChaoOrMissingIdentityBecomesAChaosMascot() throws Exception {
        assertNull(OriginalCatalog.forPlayer(null));
        for (String form : new String[]{"normal", "hero", "dark", "", "unknown"})
            assertNull(OriginalCatalog.forPlayer(player(form, -100).put("admin", true)));
        assertNull(OriginalCatalog.forPlayer(new JSONObject().put("form", 7)));
    }
    @Test public void preservesBrowserVariantPrecedenceAndAlignmentBoundaries() throws Exception {
        variant("neutral", player("chaos", 59));
        variant("neutral", player("chaos", -59));
        variant("hero", player("chaos", 60));
        variant("dark", player("chaos", -60));
        variant("hero", player("HERO-CHAOS", -100));
        variant("dark", player("dark-chaos", 0));
        variant("hero", player("dark-chaos", 100));
    }
    @Test public void onlyExplicitServerAdminFlagAddsTheWholeApprovedEmeraldIdentity() throws Exception {
        JSONObject owner = OriginalCatalog.forPlayer(player("chaos", -100).put("admin", true));
        assertEquals("/api/assets/emerald-cyan.png", owner.getString("chestGemPath"));
        assertEquals(7, owner.getJSONArray("orbit").length());
        java.util.Set<String> gems = new java.util.HashSet<>();
        for (int i = 0; i < 7; i++) {
            JSONObject gem = owner.getJSONArray("orbit").getJSONObject(i);
            assertTrue(gems.add(gem.getString("path")));
            assertTrue(gem.getString("sha256").matches("[a-f0-9]{64}"));
            assertTrue(gem.getString("path").startsWith("/api/assets/emerald-"));
        }
        for (Object flag : new Object[]{false, "true", 1, JSONObject.NULL}) {
            JSONObject regular = OriginalCatalog.forPlayer(player("chaos", -100).put("admin", flag));
            assertFalse(regular.has("chestGemPath"));
            assertFalse(regular.has("orbit"));
        }
    }
    @Test public void descriptorResultsCannotLeakMutationsBetweenPlayers() throws Exception {
        JSONObject first = OriginalCatalog.forPlayer(player("chaos", -100).put("admin", true));
        first.getJSONArray("orbit").getJSONObject(0).put("path", "/invalid.png");
        JSONObject second = OriginalCatalog.forPlayer(player("chaos", -100).put("admin", true));
        assertEquals("/api/assets/emerald-cyan.png", second.getJSONArray("orbit").getJSONObject(0).getString("path"));
    }
}
