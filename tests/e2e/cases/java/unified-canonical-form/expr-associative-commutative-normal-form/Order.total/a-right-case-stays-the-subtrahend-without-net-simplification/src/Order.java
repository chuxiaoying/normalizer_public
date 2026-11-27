class Order {
    static int total(int base, int value) {
        return base - (value > 0 ? 3 : 7);
    }
}
