class VariableLoop {
    static int count(int limit) {
        int total = 0; int exclusiveEnd = limit + 1;
        /* @normalizer-ir
           total = count from [0, exclusiveEnd) as i
        */
        return total;
    }
}
