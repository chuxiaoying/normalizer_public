class Order {
    static int total(int base, int value) {
        return (base > 0 ? 3 : 7) - (value > 0 ? 1 : 2);
    }
}
