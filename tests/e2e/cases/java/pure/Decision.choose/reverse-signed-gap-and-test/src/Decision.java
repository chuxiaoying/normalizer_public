class Decision {
    static int choose(int value, int limit, int fallback) {
        int adjusted = value + 1;
        int gap = limit - adjusted;

        if (gap < 0) {
            return adjusted;
        }
        if (gap == 0) {
            return fallback;
        }
        return adjusted - 1;
    }
}
