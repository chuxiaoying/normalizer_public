class CanonicalQuotient {
    static int remainder(int input, int divisor) {
        return input - (input / divisor) * divisor;
    }
}
