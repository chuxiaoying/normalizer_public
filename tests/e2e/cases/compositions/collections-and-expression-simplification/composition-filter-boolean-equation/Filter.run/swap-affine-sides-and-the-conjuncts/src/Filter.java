class Filter {
    static int run(int[] values, int target, boolean enabled) {
        int result;
        /* @normalizer-ir
           result = count from values as item where target - 2 == item and enabled == true
        */
        return result;
    }
}
