class CanonicalQuotient {
    static int guarded(boolean fail) {
        if (fail) return 5 / (7 / 0);
        return 7;
    }
}
