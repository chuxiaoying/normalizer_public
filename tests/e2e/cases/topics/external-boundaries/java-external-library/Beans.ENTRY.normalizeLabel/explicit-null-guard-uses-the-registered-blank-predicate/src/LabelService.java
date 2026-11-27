import normalizer.intrinsics.N;

class LabelService {
    String normalizeLabel(@N.Nullable String raw, String fallback) {
        return raw == null ? fallback : N.defaultIfBlank(
            raw,
            fallback
        );
    }
}

class Beans {
    static final LabelService ENTRY = new LabelService();
}
