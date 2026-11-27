class Decision {
    static int choose(int value, int limit, int fallback) {
        int adjusted = value + 1;
        int gap = adjusted - limit;

        if (gap > 0) {
            return adjusted;
        }
        if (gap == 0) {
            return value;
        }
        return adjusted - 1;
    }
}
