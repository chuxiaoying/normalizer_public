import normalizer.intrinsics.N;

class Ledger {
    @N.EntryState
    int balance;

    int delta(int amount) {
        int before = balance;
        balance = balance + amount;
        int midway = balance;
        balance = balance + 1;
        return balance - midway;
    }
}

class Beans {
    static final Ledger ENTRY = new Ledger();
}
