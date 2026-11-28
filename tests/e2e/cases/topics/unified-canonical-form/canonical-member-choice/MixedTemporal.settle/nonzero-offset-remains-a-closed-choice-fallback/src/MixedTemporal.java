class MixedTemporal {
    static int settle(int[] options, int[] data) {
        int acquired;
        /* @normalizer-ir acquired = first from data[1:] else 0 */
        int selected;
        /* @normalizer-ir selected = choose from options else acquired */
        return selected;
    }
}
