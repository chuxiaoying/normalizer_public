class Ratio {
    static int safeDivide(int divisor) {
        try { return 10 / divisor; }
        catch (ArithmeticException problem) { throw problem; }
    }
}
