class VariableLoop {
    static int count(int limit) {
        int total = 0;
        /* @normalizer-ir
           total = count from [0, limit) as i
        */
        return total;
    }
}
