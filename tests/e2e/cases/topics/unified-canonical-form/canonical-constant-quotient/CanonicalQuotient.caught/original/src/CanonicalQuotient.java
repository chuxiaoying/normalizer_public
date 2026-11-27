class CanonicalQuotient {
    static int caught() {
        try { return 7 % 0; }
        catch (ArithmeticException ignored) { return 11; }
    }
}
