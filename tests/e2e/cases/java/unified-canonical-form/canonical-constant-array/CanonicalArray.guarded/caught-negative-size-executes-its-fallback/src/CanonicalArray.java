class CanonicalArray {
    static int guarded(boolean fail) {
        try { return (new int[-1]).length; } catch (RuntimeException ignored) { return 42; }
    }
}
