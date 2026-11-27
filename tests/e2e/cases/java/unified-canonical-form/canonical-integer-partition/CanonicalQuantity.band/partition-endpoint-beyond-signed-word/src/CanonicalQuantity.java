class CanonicalQuantity {
    static int band(long quantity) {
        if (quantity <= 9223372036854775807L) return 10;
        if (quantity <= 9223372036854775807L + 1) return 20;
        return 30;
    }
}
