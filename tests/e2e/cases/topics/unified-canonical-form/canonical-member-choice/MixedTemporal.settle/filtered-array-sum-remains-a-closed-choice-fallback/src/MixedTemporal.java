class MixedTemporal {
    static int settle(int[] options, int[] values) {
        int total;
        /* @normalizer-ir total = sum from values as v where v > 0 yield v */
        int selected;
        /* @normalizer-ir selected = choose from options else total */
        return selected;
    }
}
