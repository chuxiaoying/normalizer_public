import normalizer.intrinsics.N;
class Quota { int units; }
class Reserve { int units; }
class Counter {
    @N.EntryState Quota zebra;
    @N.EntryState Reserve amber;

    int readPair() {
        zebra.units = 9; return amber.units;
    }
}

class Beans {
    static final Counter ENTRY = new Counter();
    static {
        ENTRY.zebra = new Quota();
        ENTRY.amber = new Reserve();
    }
}
