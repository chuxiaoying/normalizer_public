class MixedTemporal {
    static int settle(int[] values, int backup) {
        int picked;
        /* @normalizer-ir picked = choose from values else backup */
        int total;
        /* @normalizer-ir total = sum from values as item yield item + backup */
        return total + picked;
    }
}
