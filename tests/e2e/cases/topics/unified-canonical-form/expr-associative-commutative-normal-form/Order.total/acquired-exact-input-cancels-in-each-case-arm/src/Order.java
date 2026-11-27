class Order {
    static int total(int base, int value) {
        return (value > 0 ? value : value + 2) - value;
    }
}
