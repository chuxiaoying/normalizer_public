import normalizer.intrinsics.N;

class GuardWithProbe {
    @N.EntryState
    boolean stored;
    static final GuardWithProbe INSTANCE = new GuardWithProbe(new Probe());

    @N.External
    private final Probe probe;

    GuardWithProbe(Probe probe) { this.probe = probe; }

    boolean allowed(boolean active, int value) {
        stored = active && probe.read(value) > 0;
        return stored;
    }
}

class Probe { native int read(int value); }
