class Reduce {
    static int run(int[] values, boolean enabled, int initial, int step) {
        int offset = enabled ? step : 0;
        int result;
        /* @normalizer-ir
           result = sum from values as value yield value + offset
        */
        return initial + result;
    }
}
