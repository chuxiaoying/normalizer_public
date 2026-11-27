import normalizer.intrinsics.N;

class Ledger {
    @N.EntryState
    int balance;

    int delta(int amount) {
        int before = balance;
        balance = balance + amount + 1;
        return balance - before;
    }
}

class Beans {
    static final Ledger ENTRY = new Ledger();
}
