class QuotaFold {
    static int charge(int[] readings, int cutoff, int deductible) {
        int total;
        /* @normalizer-ir
           total = sum from readings as reading
               where reading > cutoff
               yield reading - deductible
        */
        return total;
    }
}
