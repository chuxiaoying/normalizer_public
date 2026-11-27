class NestedValues {
    static int total(int[][] groups, int grp) {
        int total;
        /* @normalizer-ir
           total = sum from groups as group
               yield sum from group as value yield value - grp
        */
        return total;
    }
}
