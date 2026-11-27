import normalizer.intrinsics.N;

class CanonicalCount {
    static int present(@N.Nullable String label) {
        if (label != null) return 1;
        return 0;
    }
}
