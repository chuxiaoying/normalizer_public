class CanonicalQuantity {
    static int band(double quantity) {
        if (0 >= quantity) return 10;
        if (1 > quantity) return 20;
        return 30;
    }
}
