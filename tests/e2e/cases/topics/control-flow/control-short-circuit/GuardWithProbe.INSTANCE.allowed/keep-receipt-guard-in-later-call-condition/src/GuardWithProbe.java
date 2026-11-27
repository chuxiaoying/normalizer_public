import normalizer.intrinsics.N;

class GuardWithProbe {
    static final GuardWithProbe INSTANCE = new GuardWithProbe(new Probe());

    @N.External
    private final Probe probe;

    GuardWithProbe(Probe probe) { this.probe = probe; }

    boolean allowed(boolean active, int value) {
        if (active && probe.read(value) > 0) probe.signal();
        return true;
    }
}

class Probe { native int read(int value); native void signal(); }
