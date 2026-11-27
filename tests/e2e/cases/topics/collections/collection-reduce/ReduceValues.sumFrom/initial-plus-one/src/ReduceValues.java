class ReduceValues {
    static int sumFrom(int[] values, int initial) {
        int total;
        /* @normalizer-ir
           total = reduce values from initial + 1 as (sum, value) yield sum + value
        */
        return total;
    }
}
