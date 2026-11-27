class Branch {
    static int clamp(int value, int limit) {
        if (value < 10) return 10;
        if (value > limit) return limit;
        return value;
    }
}
