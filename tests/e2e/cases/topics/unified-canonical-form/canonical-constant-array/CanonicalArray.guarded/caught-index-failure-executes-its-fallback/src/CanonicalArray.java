class CanonicalArray {
    static int guarded(boolean fail) {
        try { return (new int[]{2, 7})[-1]; } catch (RuntimeException ignored) { return 42; }
    }
}
