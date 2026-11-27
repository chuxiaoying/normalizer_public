class FirstValue {
    static int firstOrZero(int[] values) {
        int result;
        /* @normalizer-ir
           result = first from values[cutoff:] else 0
        */
        return result;
    }
}
