class MixedTemporal {
    static int settle(int[] options, int[] values) {
        int acquired;
        /* @normalizer-ir acquired = count distinct from values as v where v > 0 yield v */
        int selected;
        /* @normalizer-ir selected = choose from options else acquired */
        return selected;
    }
}
