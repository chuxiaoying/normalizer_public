class PrefixSelection {
    static int firstAbove(int[] tariff, int cutoff, int fallback) {
        int length = tariff.length;
        int end;
        /* @normalizer-ir
           end = first from [0, length) as i where tariff[i] == 0 yield i else length
        */
        int result;
        /* @normalizer-ir
           result = first from [0, end) as i where tariff[i] > cutoff yield tariff[i] else fallback
        */
        return result;
    }
}
