class NestedValues {
    static int total(int[][] groups) {
        int total = 7;
        /* @normalizer-ir
           total += sum from groups as group
               yield sum from group as value yield value
        */
        return total;
    }
}
