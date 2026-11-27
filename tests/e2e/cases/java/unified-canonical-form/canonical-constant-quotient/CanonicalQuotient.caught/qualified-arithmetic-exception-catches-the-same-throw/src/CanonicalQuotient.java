class CanonicalQuotient {
    static int caught() {
        try { return 7 % 0; }
        catch (java.lang.ArithmeticException ignored) { return 11; }
    }
}
