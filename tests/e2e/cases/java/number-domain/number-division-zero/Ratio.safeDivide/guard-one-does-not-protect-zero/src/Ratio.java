class Ratio {
    static int safeDivide(int divisor) {
        if (divisor == 1) return 0;
        return divisor / divisor;
    }

    static int unchecked(int divisor) {
        return divisor / divisor;
    }
}
