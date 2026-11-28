class ReduceValues {
    static int sumFrom(int[] values, int initial) {
        int total = initial + 1;
        /* @normalizer-ir total += sum from values as value yield value */
        return total;
    }
}
