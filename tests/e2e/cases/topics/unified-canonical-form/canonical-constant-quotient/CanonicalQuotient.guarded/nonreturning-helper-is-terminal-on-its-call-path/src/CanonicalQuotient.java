class CanonicalQuotient {
    static int guarded(boolean fail) {
        if (fail) return quotient(0);
        return 7;
    }
    static int quotient(int divisor) { return 10 / divisor; }
}
