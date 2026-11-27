class CanonicalQuantity {
    static int band(double quantity) {
        if (quantity <= 0) return 10;
        if (quantity < 1) return 20;
        return 30;
    }
}
