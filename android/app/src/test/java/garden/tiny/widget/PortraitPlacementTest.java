package garden.tiny.widget;

import org.junit.Test;
import static org.junit.Assert.*;

public final class PortraitPlacementTest {
    @Test public void allThreePinnedFrontIdleFeetLandOnTheVerifiedGrassPointWithoutSquashing() {
        for (float originalFoot : new float[]{206f, 203.833333f}) {
            float[] bounds = PortraitPlacement.onGrass(originalFoot);
            float scaleX = (bounds[2] - bounds[0]) / 256f;
            float scaleY = (bounds[3] - bounds[1]) / 256f;
            assertEquals("Uniform native proportions", scaleX, scaleY, 0.00001f);
            assertEquals("Grass x after original map center crop", 161.792f, bounds[0] + 128f * scaleX, 0.001f);
            assertEquals("Grass y, above the pond", 174.08f, bounds[1] + originalFoot * scaleY, 0.001f);
            assertTrue("Original flame remains in the scene", bounds[1] >= 0);
            assertTrue("Original front feet remain in the scene", bounds[1] + originalFoot * scaleY < 256);
        }
    }
    @Test public void invalidSourceGroundingIsRejected() {
        for (float invalid : new float[]{Float.NaN, Float.POSITIVE_INFINITY, 0, 257})
            assertThrows(IllegalArgumentException.class, () -> PortraitPlacement.onGrass(invalid));
    }
}
