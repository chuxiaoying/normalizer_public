class NestedValues {
    static int total(int[][] groups) {
        int total;
        /* @normalizer-ir
           total = sum from groups as group
               yield sum from group as value yield value
        */
        return total;
    }
}
