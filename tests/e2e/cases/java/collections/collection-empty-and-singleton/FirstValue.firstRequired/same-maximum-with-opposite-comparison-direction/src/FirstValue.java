class FirstValue {
    static int firstRequired(int[] values, int threshold) {
        int first = values[0]; return threshold > first ? threshold : first;
    }
}
