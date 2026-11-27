class Balance {
    static int addFee(int balance, int fee) {
        int adjusted = balance > 0 ? balance : balance + 2; return adjusted - balance;
    }
}
