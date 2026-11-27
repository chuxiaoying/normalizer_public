class RequestOffset { int offset; } class NestedValues {
    static int total(int[][] groups, RequestOffset request) {
        int total = request.offset + 7;
        /* @normalizer-ir
           total += sum from groups as group
               yield sum from group as value yield value
        */
        return total;
    }
}
