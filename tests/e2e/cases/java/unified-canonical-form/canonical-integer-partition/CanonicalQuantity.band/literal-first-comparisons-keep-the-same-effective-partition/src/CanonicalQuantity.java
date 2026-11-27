class CanonicalQuantity {
    static int band(int quantity) {
        if (0 > quantity) return 10;
        if (3 > quantity) return 20;
        if (5 >= quantity) return 20;
        return 30;
    }
}
