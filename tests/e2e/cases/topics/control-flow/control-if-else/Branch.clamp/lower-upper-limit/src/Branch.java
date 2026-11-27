class Branch {
    static int clamp(int value, int limit) {
        if (value < 0) return 0;
        if (value > limit + 1) return limit + 1;
        return value;
    }
}
