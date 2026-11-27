import normalizer.intrinsics.N;
class Quota { int units; }
class Counter {
    @N.EntryState Quota amber;
    @N.EntryState Quota zebra;

    int readPair() {
        return amber.units + zebra.units;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
    static {
        ENTRY.zebra = new Quota();
        ENTRY.amber = new Quota();
    }
}
