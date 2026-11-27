class CanonicalQuantity {
    static int band(int quantity) {
        if (quantity <= -1) return 10;
        if (quantity >= 0 && quantity <= 5) return 20;
        return 30;
    }
}
