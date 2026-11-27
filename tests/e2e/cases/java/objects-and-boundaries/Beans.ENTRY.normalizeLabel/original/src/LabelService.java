import normalizer.intrinsics.N;

class LabelService {
    String normalizeLabel(@N.Nullable String raw, String fallback) {
        return N.defaultIfBlank(
            N.trim(raw),
            fallback
        );
    }
}

class Beans {
    static final LabelService ENTRY = new LabelService();
}
