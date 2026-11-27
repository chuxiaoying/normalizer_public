class NestedValues {
    static int total(int[][] groups) {
        int total;
        /* @normalizer-ir
           total = sum from groups as group
               yield sum from group as value where value > 0 yield value
        */
        return total;
    }
}
