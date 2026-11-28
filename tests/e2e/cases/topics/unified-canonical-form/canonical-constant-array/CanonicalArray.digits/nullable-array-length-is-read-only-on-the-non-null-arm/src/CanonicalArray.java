import normalizer.intrinsics.N;

class CanonicalArray {
    static int digits(@N.Nullable int[] values) {
        return values == null ? 0 : values.length;
    }
}
