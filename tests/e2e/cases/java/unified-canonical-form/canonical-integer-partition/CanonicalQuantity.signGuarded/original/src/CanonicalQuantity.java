class CanonicalQuantity {
    static int signGuarded(int quantity) {
        if (quantity > 0) return 1;
        if (quantity < 0) return -1;
        throw new RuntimeException("zero");
    }
}
