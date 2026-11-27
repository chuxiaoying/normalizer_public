class Early {
    static int choose(boolean useFallback, int value, int fallback) {
        if (useFallback) return fallback;
        return -value;
    }
}
