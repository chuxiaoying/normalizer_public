class MixedTemporal {
    static int settle(int[] values, int backup) {
        int total;
/* @normalizer-ir total = reduce values from backup as (acc, item) yield acc * 2 + item */
int picked;
/* @normalizer-ir picked = choose from values else backup */
        return total + picked;
    }
}
