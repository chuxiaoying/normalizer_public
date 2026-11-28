class MixedTemporal {
    static int settle(int[] values, int backup, int[] candidates) {
        int picked;
        /* @normalizer-ir picked = choose from values else backup */
        int total;
        /* @normalizer-ir total = sum from values as item yield item + picked */
        int chosenAfterFold;
/* @normalizer-ir chosenAfterFold = choose from candidates else total */
return chosenAfterFold;
    }
}
