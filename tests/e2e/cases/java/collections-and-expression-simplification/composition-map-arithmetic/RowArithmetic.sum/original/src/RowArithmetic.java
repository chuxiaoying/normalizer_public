class RowArithmetic {
    static int sum(int[] values, int extra) {
        int total;
        /* @normalizer-ir
           total = sum from values as value yield (value + extra) - extra
        */
        return total;
    }
}
