class Sequence {
    static int next(int current) {
        if (current == 2147483647) return -2147483648;
        return current + 1;
    }
}
