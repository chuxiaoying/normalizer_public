import normalizer.intrinsics.N;

class CanonicalCount {
    static int present(@N.Nullable String label) {
        if (label == null) return 0;
        return 2;
    }
}
