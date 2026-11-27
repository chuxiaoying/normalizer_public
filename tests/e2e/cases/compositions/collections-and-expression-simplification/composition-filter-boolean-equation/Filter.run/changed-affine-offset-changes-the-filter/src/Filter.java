class Filter {
    static int run(int[] values, int target, boolean enabled) {
        int result;
        /* @normalizer-ir
           result = count from values as item where enabled == true and item + 3 == target
        */
        return result;
    }
}
