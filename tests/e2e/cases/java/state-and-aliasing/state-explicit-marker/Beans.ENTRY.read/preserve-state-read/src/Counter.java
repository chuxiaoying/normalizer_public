import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int read() {
        int result = this.value + 1;
        return result;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
