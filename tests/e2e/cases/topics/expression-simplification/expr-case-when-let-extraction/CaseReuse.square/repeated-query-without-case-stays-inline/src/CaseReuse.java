class CaseReuse {
    static int square(int[] values, boolean enabled, int extra) {
        int offset = extra;
        int total;
        /* @normalizer-ir
           total = sum from values as value yield value + offset
        */
        return total * total;
    }
}
