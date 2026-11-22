package garden.tiny.widget;

import android.content.ComponentName;
import android.content.Context;
import android.content.pm.ActivityInfo;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.content.pm.ServiceInfo;
import android.content.res.Configuration;
import android.os.SystemClock;
import android.app.Instrumentation;
import androidx.test.platform.app.InstrumentationRegistry;
import junit.framework.TestCase;
import android.view.View;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.ImageView;
import android.widget.RemoteViews;
import android.widget.TextView;

import java.util.Arrays;
import java.util.concurrent.atomic.AtomicReference;
import org.json.JSONArray;
import org.json.JSONObject;

/** Executes native RemoteViews on Android, separately from the browser preview. */
@SuppressWarnings("deprecation")
public final class WidgetLayoutTest extends TestCase {
    private Instrumentation getInstrumentation() { return InstrumentationRegistry.getInstrumentation(); }
    private static final int[] IDS = { R.id.widget_root, R.id.widget_name, R.id.widget_form,
            R.id.widget_avatar, R.id.widget_status, R.id.widget_needs, R.id.widget_stats,
            R.id.widget_activity, R.id.widget_training, R.id.widget_sync, R.id.widget_pet,
            R.id.widget_feed, R.id.widget_open, R.id.widget_refresh };

    public void testCompactAppliesAllIdsAndKeepsCareDisabledBeforePairing() throws Exception {
        checkLayout(R.layout.widget_compact, 160, 180, false, 1f);
    }

    public void testWideAppliesAllIdsAndKeepsCareDisabledBeforePairing() throws Exception {
        checkLayout(R.layout.widget_wide, 320, 180, false, 1f);
    }

    public void testLargePreservesSevenStatsAndHasNoClippedControls() throws Exception {
        checkLayout(R.layout.widget_large, 320, 360, true, 1f);
    }

    public void testLargestValidStringsAtThirtyPercentLargerFont() throws Exception {
        checkLayout(R.layout.widget_compact, 160, 180, true, 1.3f);
        checkLayout(R.layout.widget_wide, 320, 180, true, 1.3f);
        checkLayout(R.layout.widget_large, 320, 360, true, 1.3f);
    }

    private static JSONObject fixture() throws Exception {
        JSONObject stats = new JSONObject();
        for (int i = 0; i < Snapshot.SKILLS.length; i++) {
            stats.put(Snapshot.SKILLS[i], new JSONObject().put("level", 99).put("grade", i < 5 ? 5 : 255).put("points", 9999));
        }
        JSONObject snapshot = new JSONObject().put("protocol", Snapshot.PROTOCOL).put("revision", 99)
                .put("serverTimeMs", 1700000000000L).put("lastSavedAtMs", 1700000000000L)
                .put("chao", new JSONObject().put("id", "test-only-chao").put("name", "Test fixture with deliberately long name for truncation checks")
                        .put("form", "dark-chaos").put("rings", 9007199254740991L)
                        .put("needs", new JSONObject().put("energy", 100).put("fullness", 100).put("happiness", 100)).put("stats", stats))
                .put("activity", new JSONObject().put("label", "Exploring the garden with the longest safe activity label for text bounds and accessibility checks"))
                .put("training", new JSONObject().put("status", "training").put("skill", "stamina").put("endAtMs", 1700000000001L))
                .put("care", new JSONObject().put("actions", new JSONArray()));
        return new JSONObject().put("snapshot", snapshot).put("token", "test-only-token-never-sent")
                .put("origin", "https://garden.example").put("chaoId", "test-only-chao")
                .put("process", GardenRepository.PROCESS).put("receivedElapsed", SystemClock.elapsedRealtime());
    }

    private void checkLayout(int layoutId, int widthDp, int heightDp, boolean pairedFixture, float fontScale) throws Exception {
        AtomicReference<Throwable> error = new AtomicReference<>();
        getInstrumentation().runOnMainSync(() -> {
            try {
                Context target = getInstrumentation().getTargetContext();
                Configuration configuration = new Configuration(target.getResources().getConfiguration());
                configuration.fontScale = fontScale;
                Context context = target.createConfigurationContext(configuration);
                // Deterministic test-only projection. Never writes storage or contacts the server.
                RemoteViews views = GardenWidget.views(context, layoutId, pairedFixture ? fixture() : new JSONObject());
                FrameLayout host = new FrameLayout(context);
                View content = views.apply(context, host);
                host.addView(content, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
                float density = context.getResources().getDisplayMetrics().density;
                int width = Math.round(widthDp * density), height = Math.round(heightDp * density);
                host.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
                host.layout(0, 0, width, height);
                for (int id : IDS) assertNotNull("Required widget ID missing: " + id, content.findViewById(id));
                assertTrue(content.findViewById(R.id.widget_avatar) instanceof ImageView);
                assertTrue(content.findViewById(R.id.widget_open) instanceof Button);
                assertFalse("Unpaired widget must not enable pet", content.findViewById(R.id.widget_pet).isEnabled());
                assertFalse("Unpaired widget must not enable feed", content.findViewById(R.id.widget_feed).isEnabled());
                for (int id : new int[]{R.id.widget_pet, R.id.widget_feed, R.id.widget_open, R.id.widget_refresh}) {
                    View button = content.findViewById(id);
                    if (button.getVisibility() == View.VISIBLE) {
                        assertTrue("Visible action must retain a 48dp touch height", button.getHeight() >= Math.round(48 * density));
                        assertTrue("Visible action must retain a 48dp touch width", button.getWidth() >= Math.round(48 * density));
                    }
                }
                if (pairedFixture && layoutId == R.layout.widget_large) {
                    TextView stats = content.findViewById(R.id.widget_stats);
                    String accessibleStats = stats.getText() + " " + stats.getContentDescription();
                    for (String skill : Snapshot.SKILLS) assertTrue("Seven-stat accessible display lost " + skill, accessibleStats.toLowerCase(java.util.Locale.ROOT).contains(skill));
                    assertTrue("Seven stat values should fit in four rows", stats.getLayout().getLineCount() <= 4);
                    for (int line = 0; line < stats.getLayout().getLineCount(); line++)
                        assertEquals("Stats should fit without truncation", 0, stats.getLayout().getEllipsisCount(line));
                    assertTrue("Stat text must remain accessible", stats.getImportantForAccessibility() != View.IMPORTANT_FOR_ACCESSIBILITY_NO);
                }
                checkVisibleBounds(content);
            } catch (Throwable failure) { error.set(failure); }
        });
        if (error.get() != null) throw new AssertionError("Native widget layout failed", error.get());
    }

    private static void checkVisibleBounds(View view) {
        if (view.getVisibility() != View.VISIBLE) return;
        assertTrue("Visible widget child has no width: " + view.getId(), view.getWidth() > 0);
        assertTrue("Visible widget child has no height: " + view.getId(), view.getHeight() > 0);
        if (view instanceof TextView) {
            TextView text = (TextView) view;
            if (text.getLayout() != null && text.getLayout().getLineCount() > 0) {
                int textHeight = text.getLayout().getLineBottom(text.getLayout().getLineCount() - 1);
                assertTrue("Visible text clipped vertically: " + view.getId(), textHeight <= text.getHeight() - text.getCompoundPaddingTop() - text.getCompoundPaddingBottom());
            }
        }
        if (view instanceof ViewGroup) {
            ViewGroup parent = (ViewGroup) view;
            for (int i = 0; i < parent.getChildCount(); i++) {
                View child = parent.getChildAt(i);
                if (child.getVisibility() != View.VISIBLE) continue;
                assertTrue("Widget child extends left of parent", child.getLeft() >= 0);
                assertTrue("Widget child extends above parent", child.getTop() >= 0);
                assertTrue("Widget child extends beyond parent width: " + child.getId(), child.getRight() <= parent.getWidth());
                assertTrue("Widget child extends beyond parent height: " + child.getId(), child.getBottom() <= parent.getHeight());
                checkVisibleBounds(child);
            }
        }
    }

    public void testPackageUsesNetworkOnlyAndProtectsPrivateEntryPoints() throws Exception {
        Context context = getInstrumentation().getTargetContext();
        PackageManager manager = context.getPackageManager();
        PackageInfo info = manager.getPackageInfo(context.getPackageName(), PackageManager.GET_PERMISSIONS);
        assertNotNull(info.requestedPermissions);
        assertTrue(Arrays.asList(info.requestedPermissions).contains("android.permission.INTERNET"));
        for (String permission : info.requestedPermissions) {
            assertTrue("Unexpected Android permission: " + permission,
                    permission.equals("android.permission.INTERNET") || permission.equals("android.permission.ACCESS_NETWORK_STATE")
                    // Not in the manifest: an Android 17 phone listed it for this app by itself (cause unverified). Still network only.
                    || permission.equals("android.permission.ACCESS_LOCAL_NETWORK"));
        }
        assertEquals(0, info.applicationInfo.flags & ApplicationInfo.FLAG_USES_CLEARTEXT_TRAFFIC);
        assertEquals(0, info.applicationInfo.flags & ApplicationInfo.FLAG_ALLOW_BACKUP);
        ActivityInfo careReceiver = manager.getReceiverInfo(new ComponentName(context, WidgetActionReceiver.class), 0);
        assertFalse("Another app must not broadcast care commands", careReceiver.exported);
        ServiceInfo job = manager.getServiceInfo(new ComponentName(context, GardenJobService.class), 0);
        assertEquals("android.permission.BIND_JOB_SERVICE", job.permission);
    }
}
