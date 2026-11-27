class Reduce {
    static int run(int[] values, boolean enabled, int initial, int step) {
        int offset = step;
        int result;
        /* @normalizer-ir
           result = reduce values from initial as (acc, item) yield acc * 2 + item + offset
        */
        return result * result;
    }
}
