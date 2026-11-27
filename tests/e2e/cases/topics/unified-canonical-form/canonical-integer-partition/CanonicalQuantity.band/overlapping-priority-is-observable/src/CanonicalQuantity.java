class CanonicalQuantity {
    static int band(int quantity) {
        if (quantity < 3) return 20;
        if (quantity < 0) return 10;
        if (quantity <= 5) return 20;
        return 30;
    }
}
