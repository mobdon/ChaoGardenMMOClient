package garden.tiny.widget;

/** Presentation only: source-foot placement over the center-cropped original garden image. */
final class PortraitPlacement {
    private PortraitPlacement() { }
    static float[] onGrass(float sourceFootY) {
        if (!Float.isFinite(sourceFootY) || sourceFootY < 1 || sourceFootY > 256)
            throw new IllegalArgumentException("Invalid source artwork grounding");
        // Original grass point (109.12,108.8) on the 176x160 map, cropped 8px at either side.
        float left = 256f * 0.632f - 128f * 0.75f;
        float top = 256f * 0.68f - sourceFootY * 0.75f;
        return new float[]{left, top, left + 192f, top + 192f};
    }
}
