class Branch {
    static int clamp(int value, int limit) {
        if (value < 0) return 0;
        if (value > limit) return limit;
        return value;
    }
}
