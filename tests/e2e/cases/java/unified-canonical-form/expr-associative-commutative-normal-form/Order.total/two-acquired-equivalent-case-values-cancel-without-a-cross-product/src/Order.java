class Order {
    static int total(int base, int value) {
        int left = value > 0 ? value + base : value * base;
        int right = value > 0 ? base + value : base * value;
        return left - right;
    }
}
