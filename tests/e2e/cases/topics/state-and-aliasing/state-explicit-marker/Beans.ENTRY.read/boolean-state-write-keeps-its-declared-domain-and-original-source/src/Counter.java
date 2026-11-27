import normalizer.intrinsics.N;

class Counter {
    @N.EntryState
    boolean value;

    boolean read() {
        this.value = !this.value; return this.value;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
}
