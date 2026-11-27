class Balance {
    static double addFee(int balance, int fee) {
        return (balance > 0 ? 0.10 : 1.0) - (balance > 0 ? .1 : 1);
    }
}
