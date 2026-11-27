class NestedValues {
    static int total(int[][] groups, int value) {
        int total;
        /* @normalizer-ir
           total = sum from groups as group
               yield sum from group as item yield item - value
        */
        return total;
    }
}
