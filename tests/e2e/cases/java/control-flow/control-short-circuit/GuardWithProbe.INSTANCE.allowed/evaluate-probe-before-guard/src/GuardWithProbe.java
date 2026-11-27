import normalizer.intrinsics.N;

class GuardWithProbe {
    static final GuardWithProbe INSTANCE = new GuardWithProbe(new Probe());

    @N.External
    private final Probe probe;

    GuardWithProbe(Probe probe) { this.probe = probe; }

    boolean allowed(boolean active, int value) {
        int observed = probe.read(value);
        return active && observed > 0;
    }
}

class Probe { native int read(int value); }
