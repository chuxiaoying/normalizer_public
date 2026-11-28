class VariableLoop {
    static int count() {
        int n;
        /* @normalizer-ir n = count from [3, 2) as i where i < 4 */
        return n;
    }
}
