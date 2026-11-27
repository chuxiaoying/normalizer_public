class CaseReuse {
    static int square(int[] values, boolean enabled, int extra) {
        int offset = enabled ? extra : 0;
        int total;
        /* @normalizer-ir
           total = sum from values as value yield value + offset
        */
        int reversedOffset = !enabled ? 0 : extra;
        int second;
        /* @normalizer-ir
           second = sum from values as value yield value + reversedOffset
        */
        return total * second;
    }
}
