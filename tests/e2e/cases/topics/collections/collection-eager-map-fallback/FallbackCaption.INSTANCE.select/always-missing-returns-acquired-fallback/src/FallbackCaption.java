import normalizer.intrinsics.N;

class CaptionMap {}
class CaptionUnavailable extends RuntimeException {}
class CaptionService {
    native String get() throws CaptionUnavailable;
}
class FallbackCaption {
    static final FallbackCaption INSTANCE = new FallbackCaption(new CaptionService());

    @N.External private final CaptionService defaults;
    FallbackCaption(CaptionService defaults) { this.defaults = defaults; }

    @N.Nullable String select(boolean cleared) {
        CaptionMap captions;
        /* @normalizer-ir captions = value.map_empty(text, text?) */
        if (false) {
            /* @normalizer-ir captions = value.map_put(captions, "main", null) */
        }
        String fallback = defaults.get();
        @N.Nullable String selected;
        /* @normalizer-ir selected = value.map_get_or(captions, "main", fallback) */
        return selected;
    }
}
