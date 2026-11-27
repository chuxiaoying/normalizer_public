class NestedValues {
    static int total(int[][] groups) {
        int total = 0;
        /* @normalizer-ir
           total += sum from groups as group
               yield sum from group as value yield value
        */
        return total;
    }
}
