import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    int value;

    int increment() {
        value = value + 1;
        return value;
    }

    int read() {
        return value;
    }

    int incrementThenRead() {
        int before = read();
        increment();
        return before;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
