import normalizer.intrinsics.N;

class GuardWithProbe {
    static final GuardWithProbe INSTANCE = new GuardWithProbe(new Probe());

    @N.External
    private final Probe probe;

    GuardWithProbe(Probe probe) { this.probe = probe; }

    boolean allowed(boolean active, int value) {
        boolean allowed = active || probe.read(value) > 0;
        probe.publish(allowed);
        return allowed;
    }
}

class Probe { native int read(int value); native void publish(boolean allowed); }
