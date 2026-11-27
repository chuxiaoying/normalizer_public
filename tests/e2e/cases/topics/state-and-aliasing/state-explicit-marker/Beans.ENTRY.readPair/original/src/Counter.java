import normalizer.intrinsics.N;
class Quota { int units; }
class Counter {
    @N.EntryState Quota zebra;
    @N.EntryState Quota amber;

    int readPair() {
        return zebra.units + amber.units;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
    static {
        ENTRY.zebra = new Quota();
        ENTRY.amber = new Quota();
    }
}
