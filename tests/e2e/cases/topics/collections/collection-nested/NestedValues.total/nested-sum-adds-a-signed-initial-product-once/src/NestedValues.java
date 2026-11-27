class NestedValues {
    static int total(int[][] groups, int base, int multiplier) {
        int total = base * multiplier;
        /* @normalizer-ir
           total += sum from groups as group
               yield sum from group as value yield value
        */
        return total;
    }
}
