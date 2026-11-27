import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int increment() {
        value = value + 1;
        return value;
    }

    int read() {
        return value + 1;
    }

    int incrementThenRead() {
        increment();
        return read();
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
