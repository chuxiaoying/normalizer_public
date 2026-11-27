import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int increment() {
        value = value + 1;
        return value - 1;
    }

    int read() {
        return value;
    }

    int incrementThenRead() {
        increment();
        return read();
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
