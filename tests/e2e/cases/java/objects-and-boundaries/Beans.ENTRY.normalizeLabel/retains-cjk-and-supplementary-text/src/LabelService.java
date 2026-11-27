import normalizer.intrinsics.N;

class LabelService {
    String normalizeLabel(@N.Nullable String raw, String fallback) {
        return N.defaultIfBlank(
            N.trim("  上海🙂  "),
            fallback
        );
    }
}

class Beans {
    static final LabelService ENTRY = new LabelService();
}
