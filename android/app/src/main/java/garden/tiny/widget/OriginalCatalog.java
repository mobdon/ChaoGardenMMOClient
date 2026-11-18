package garden.tiny.widget;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import java.util.Locale;

/** Pinned original-art descriptors for bundled PNGs and same-origin fallback. */
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
            "414f1b7dd3396faac9661b4bcd6e7059fa18625eac50a586fce5e3c21141b497",
            "1effc4358f9fc3b0bfc3a4dbaa459a7faf21f94a34fde37e873a165cfafa22fd",
            "f275654fbbebb2d0f208dcfc9942769c87722e11e6a77679186c288184c4f721",
            "e9bed916c89c4e4203723a2dc2cd4235fce3540b2c1d43cbf2ebe46f64cb4b6b",
            "e0f28259c33db12c1d64d7dc4250ab19d9d07538ceeb89575ec7f513b1644b95",
            "126d5cb23478156d899631fdb7dac1e831709b5f4bbf78f191a7b67e9fdacdfe",
            "df7d3d8f0ceaf08d076baf5cf37eaf14c6d46f6700158bf55e75e59a55075e5a"
    };

    /** Canonical server player only; a normal or unknown form never becomes a mascot. */
    public static JSONObject forPlayer(JSONObject canonicalPlayer) throws JSONException {
        if (canonicalPlayer == null || !(canonicalPlayer.opt("form") instanceof String)) return null;
        String form = canonicalPlayer.getString("form").toLowerCase(Locale.ROOT);
        if (!form.contains("chaos")) return null;
        Object rawAlignment = canonicalPlayer.opt("alignment");
        double alignment = rawAlignment instanceof Number ? ((Number) rawAlignment).doubleValue() : 0;
        // Matches the Tiny Garden browser's chaosVariant precedence exactly.
        int variant = form.contains("hero") ? 1 : form.contains("dark") ? 2
                : form.contains("neutral") ? 0 : alignment >= 60 ? 1 : alignment <= -60 ? 2 : 0;
        return appearance(variant, Boolean.TRUE.equals(canonicalPlayer.opt("admin")));
    }

    /** An artwork example only: no player ID, account, stats, rings or granted actions. */
    public static JSONObject artPreview() throws JSONException { return appearance(2, false); }

    public static boolean canPreview(JSONObject state) {
        return state != null && !state.has("token") && !state.has("snapshot");
    }

    /** Maps only this catalog's exact pinned assets to safe packaged basenames. */
    public static String localFile(JSONObject descriptor) throws JSONException {
        String hash = descriptor.getString("sha256");
        if (descriptor.has("assetFile")) {
            if (descriptor.has("path")) throw new IllegalArgumentException("Bundled art cannot invent a server route");
            String filename = descriptor.getString("assetFile");
            for (int i = 0; i < GEM_COLORS.length; i++)
                if (filename.equals("widget-emerald-" + GEM_COLORS[i] + ".png") && hash.equals(GEM_HASHES[i])) return filename;
            throw new IllegalArgumentException("Unrecognized bundled artwork descriptor");
        }
        String path = descriptor.getString("path");
        for (int i = 0; i < VARIANTS.length; i++)
            if (path.equals("/api/assets/tiny-" + VARIANTS[i] + "-chaos.png") && hash.equals(ATLAS_HASHES[i]))
                return "tiny-" + VARIANTS[i] + "-chaos.png";
        if (path.equals("/api/assets/chao-flame.png") && hash.equals("f90fba571d8bbd3c9e184961f75f75ba9c91b47eec46601e3515410752393952"))
            return "chao-flame.png";
        throw new IllegalArgumentException("Unrecognized original artwork descriptor");
    }

    private static JSONObject appearance(int variant, boolean admin) throws JSONException {
        JSONObject appearance = new JSONObject()
                .put("atlasPath", "/api/assets/tiny-" + VARIANTS[variant] + "-chaos.png")
                .put("atlasSha256", ATLAS_HASHES[variant])
                .put("frameSize", 96).put("columns", 8).put("frameIndex", 0);
        if (variant == 2) appearance.put("flamePath", "/api/assets/chao-flame.png")
                .put("flameSha256", "f90fba571d8bbd3c9e184961f75f75ba9c91b47eec46601e3515410752393952");
        if (admin) {
            appearance.put("chestGemAsset", "widget-emerald-cyan.png").put("chestGemSha256", GEM_HASHES[0]);
            JSONArray orbit = new JSONArray();
            for (int i = 0; i < GEM_COLORS.length; i++) orbit.put(new JSONObject()
                    .put("assetFile", "widget-emerald-" + GEM_COLORS[i] + ".png").put("sha256", GEM_HASHES[i]));
            appearance.put("orbit", orbit);
        }
        return appearance;
    }
}
