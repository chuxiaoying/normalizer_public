import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int read() {
        int value = 9; return this.value + 1;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
