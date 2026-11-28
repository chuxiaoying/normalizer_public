class DistinctArrays {
    static int run(int[] a, int[] b) {
        if (a == b) return 0;
        b[0] = 12;
        int total;
        /* @normalizer-ir total = sum from [0, length(a)) as i yield a[i] */
        return total;
    }
}
