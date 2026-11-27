class Reduce {
    static int run(int[] values, boolean enabled, int initial, int step) {
        int delta = !enabled ? 0 : step;
        int result;
        /* @normalizer-ir
           result = reduce values from initial as (acc, item) yield acc * 2 + item + delta
        */
        return result * result;
    }
}
