package garden.tiny.widget;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import java.util.Locale;

/** Pinned, hash-checked local original-art routes. Contains metadata, never source art. */
public final class OriginalCatalog {
    private OriginalCatalog() { }
    private static final String[] VARIANTS = {"neutral", "hero", "dark"};
    private static final String[] ATLAS_HASHES = {
            "16f703550b4c8fc103156095e5d2ed6f5ca966fa5814377b2a4b333333801a92",
            "e30110f7b3e74cf6f53eec5b508564e47e3a2d62e3a0db657e6a610218ced708",
            "035f532da8e1b21a11192b2fb6cc301f8d18224bf4d38258aa127f1e74e9fc85"
    };
    private static final String[] GEM_COLORS = {"cyan", "blue", "yellow", "red", "green", "purple", "white"};
    private static final String[] GEM_HASHES = {
            "a6f3fe23d1962e8871a513f34f10ab53e8f3556cf21e2ec0256a67f5537f1ca1",
            "21d7b47a691819840d14c8d7e383583ec6e727037b347924339d84cc1344b833",
            "ece5c5d96158eba56009c4dac3369c56de7bcd5f90a32813641d0b67b599e835",
            "157bfdfff6064f142330bd0bfe9cadf8483851b691fc7e683861121092f0b5e1",
            "ac27cedd76e1fca87a4bd9d6fba1c9aa6800a867a0a4e20b7c8b3f3795b1208d",
            "a05bc333b6c0d4fa943f951d5af997fdcfae1c492c83e61dc57d02c4ca3812a8",
            "fbe877077a5d24e0e49459b095d6a93831e130807ba643315b06c7421eb20440"
    };

    /** Canonical server player only; a normal or unknown form never becomes a mascot. */
    public static JSONObject forPlayer(JSONObject canonicalPlayer) throws JSONException {
        if (canonicalPlayer == null || !(canonicalPlayer.opt("form") instanceof String)) return null;
        String form = canonicalPlayer.getString("form").toLowerCase(Locale.ROOT);
        if (!form.contains("chaos")) return null;
        Object rawAlignment = canonicalPlayer.opt("alignment");
        double alignment = rawAlignment instanceof Number ? ((Number) rawAlignment).doubleValue() : 0;
        // Matches the Tiny Garden browser's chaosVariant precedence exactly.
        int variant = form.contains("hero") || alignment >= 60 ? 1
                : form.contains("dark") || alignment <= -60 ? 2 : 0;
        JSONObject appearance = new JSONObject()
                .put("atlasPath", "/api/assets/tiny-" + VARIANTS[variant] + "-chaos.png")
                .put("atlasSha256", ATLAS_HASHES[variant])
                .put("frameSize", 96).put("columns", 8).put("frameIndex", 0);
        if (variant == 2) appearance.put("flamePath", "/api/assets/chao-flame.png")
                .put("flameSha256", "f90fba571d8bbd3c9e184961f75f75ba9c91b47eec46601e3515410752393952");
        if (Boolean.TRUE.equals(canonicalPlayer.opt("admin"))) {
            appearance.put("chestGemPath", "/api/assets/emerald-cyan.png").put("chestGemSha256", GEM_HASHES[0]);
            JSONArray orbit = new JSONArray();
            for (int i = 0; i < GEM_COLORS.length; i++) orbit.put(new JSONObject()
                    .put("path", "/api/assets/emerald-" + GEM_COLORS[i] + ".png").put("sha256", GEM_HASHES[i]));
            appearance.put("orbit", orbit);
        }
        return appearance;
    }
}
