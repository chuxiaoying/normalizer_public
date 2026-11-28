class PrefixSelection {
    static int firstAbove(int[] tariff, int cutoff, int fallback) {
        int length = tariff.length;
        int result;
        /* @normalizer-ir
           result = first from [0, length + 1) as i where tariff[i] > cutoff yield tariff[i] else fallback
        */
        return result;
    }
}
