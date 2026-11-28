import normalizer.intrinsics.N;

class RangeLength {
    static int run(@N.Nullable int[] values) {
        if (values == null) return 0;
        int total;
        /* @normalizer-ir total = count from [0, min(length(values), length(values))) */
        return total;
    }
}
