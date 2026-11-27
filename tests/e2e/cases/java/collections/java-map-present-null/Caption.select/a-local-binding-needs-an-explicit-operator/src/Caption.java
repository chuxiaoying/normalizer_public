import normalizer.intrinsics.N;

class CaptionMap {}
class Caption {
    @N.Nullable
    static String select(boolean cleared, String fallback) {
        CaptionMap captions;
        /* @normalizer-ir captions value.map_empty(text, text?) */
        if (cleared) {
            /* @normalizer-ir captions = value.map_put(captions, "main", null) */
        }
        @N.Nullable String selected;
        /* @normalizer-ir selected = value.map_get_or(captions, "main", fallback) */
        return selected;
    }
}
