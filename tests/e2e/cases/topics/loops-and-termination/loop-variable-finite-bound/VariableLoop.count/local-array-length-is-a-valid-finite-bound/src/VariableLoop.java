class VariableLoop {
    static int count() {
        int[] values = {2, 3};
        int n;
        /* @normalizer-ir n = count from [0, length(values)) */
        return n;
    }
}
