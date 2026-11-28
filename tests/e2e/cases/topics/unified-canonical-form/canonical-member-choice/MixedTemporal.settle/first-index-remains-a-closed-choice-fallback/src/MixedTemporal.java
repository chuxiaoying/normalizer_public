class MixedTemporal {
    static int settle(int[] options, int hi) {
        int acquired;
        /* @normalizer-ir acquired = first from [1, hi) as i where i > 2 yield i else 0 */
        int selected;
        /* @normalizer-ir selected = choose from options else acquired */
        return selected;
    }
}
