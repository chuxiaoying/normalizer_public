class MixedTemporal {
    static int settle(int[] values, int backup) {
        int picked;
        /* @normalizer-ir picked = choose from values else backup */
        int total;
        /* @normalizer-ir total = reduce values from picked as (acc, item) yield acc * 2 + item */
        int chosenAfterFold;
/* @normalizer-ir chosenAfterFold = choose from values else total */
return chosenAfterFold;
    }
}
