package garden.tiny.widget;

/** Presentation only: source-foot placement over the center-cropped original garden image. */
final class PortraitPlacement {
    private PortraitPlacement() { }
    static float[] onGrass(float sourceFootY) {
        if (!Float.isFinite(sourceFootY) || sourceFootY < 1 || sourceFootY > 256)
            throw new IllegalArgumentException("Invalid source artwork grounding");
        // Original grass point (109.12,108.8) on the 176x160 map, cropped 8px at either side.
        float scale = 0.20f;
        float side = 256f * scale;
        float left = 256f * 0.632f - 128f * scale;
        float top = 256f * 0.68f - sourceFootY * scale;
        return new float[]{left, top, left + side, top + side};
    }
}
