class CaseReuse {
    static int square(int[] values, boolean enabled, int extra) {
        int shifted = enabled ? extra : 0;
        int aggregate;
        /* @normalizer-ir
           aggregate = sum from values as value yield value + shifted
        */
        return aggregate * aggregate;
    }
}
