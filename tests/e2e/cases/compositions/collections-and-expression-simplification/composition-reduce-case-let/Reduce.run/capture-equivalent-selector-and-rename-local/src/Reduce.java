class Reduce {
    static int run(int[] values, boolean enabled, int initial, int step) {
        int delta = !enabled ? 0 : step;
        int result;
        /* @normalizer-ir
           result = sum from values as value yield value + delta
        */
        return initial + result * result;
    }
}
