import normalizer.intrinsics.N;
class InlineProbe { native int read(int value); }
class CanonicalArray {
    static final CanonicalArray INSTANCE = new CanonicalArray(new InlineProbe());
    @N.External private final InlineProbe probe;
    CanonicalArray(InlineProbe probe) { this.probe = probe; }
    static int guarded(boolean fail) { return INSTANCE.readGuarded(fail); }
    int readGuarded(boolean fail) {
        try { return (new int[]{probe.read(10), fail ? (new int[]{2})[1] : 5, probe.read(30)}).length; } catch (ArrayIndexOutOfBoundsException ignored) { return 42; }
    }
}
