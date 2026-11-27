class CanonicalArray {
    static int guarded(boolean fail) {
        try { return (new int[-1]).length; } catch (NegativeArraySizeException ignored) { return 23; }
    }
}
