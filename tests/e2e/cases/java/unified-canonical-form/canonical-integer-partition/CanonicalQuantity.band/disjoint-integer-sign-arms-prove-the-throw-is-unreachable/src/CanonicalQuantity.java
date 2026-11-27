class CanonicalQuantity {
    static int band(int quantity) {
        if (quantity > 0) return 1; if (quantity < 0) return -1; if (quantity == 0) return 0; throw new RuntimeException("unreachable");
    }
}
