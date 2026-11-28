import normalizer.intrinsics.N;

class Counter {
    @N.EntryState int value;

    void incrementIfRequested(boolean apply, int amount) {
        if (!apply) {
            return;
        }
        this.value = amount + this.value;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
