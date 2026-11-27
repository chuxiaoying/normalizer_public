import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int read() {
        this.value = this.value + 1;
        return this.value;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
