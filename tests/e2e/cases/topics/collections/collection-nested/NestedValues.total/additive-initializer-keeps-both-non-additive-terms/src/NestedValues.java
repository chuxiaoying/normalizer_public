class NestedValues {
    static int total(int[][] groups, int base, int offset) {
        int total = base * 2 + offset * 3;
        /* @normalizer-ir
           total += sum from groups as group
               yield sum from group as value yield value
        */
        return total;
    }
}
