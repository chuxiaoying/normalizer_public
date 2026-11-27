class CanonicalQuantity {
    static int signGuarded(int quantity) {
        if (0 < quantity) return 1;
        if (quantity < 0) return -1;
        throw new RuntimeException("zero");
    }
}
