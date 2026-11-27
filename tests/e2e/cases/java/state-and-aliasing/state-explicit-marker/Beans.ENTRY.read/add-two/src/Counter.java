import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int read() {
        return this.value + 2;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
