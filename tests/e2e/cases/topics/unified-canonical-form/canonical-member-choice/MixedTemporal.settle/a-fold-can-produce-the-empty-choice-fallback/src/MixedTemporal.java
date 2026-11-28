class MixedTemporal {
    static int settle(int[] values, int backup, int[] candidates) {
        int total;
/* @normalizer-ir total = sum from values as item yield item + backup */
int picked;
/* @normalizer-ir picked = choose from candidates else total */
        return total + picked;
    }
}
