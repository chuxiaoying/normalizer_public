class CanonicalQuantity {
    static int band(int quantity) {
        if (quantity < 0) return 10;
        if (quantity >= 0 && quantity <= 5) return 20;
        if (quantity >= 6) return 30;
        return 99;
    }
}
