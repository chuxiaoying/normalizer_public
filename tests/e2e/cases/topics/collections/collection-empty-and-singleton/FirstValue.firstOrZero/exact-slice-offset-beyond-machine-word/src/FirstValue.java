class FirstValue {
    static int firstOrZero(int[] values) {
        int result;
        /* @normalizer-ir
           result = first from values[18446744073709551616:] else 0
        */
        return result;
    }
}
