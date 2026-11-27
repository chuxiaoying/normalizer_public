class CanonicalArray {
    static int guarded(boolean fail) {
        try { return (new int[]{2, 7})[2]; } catch (java.lang.IndexOutOfBoundsException ignored) { return 19; }
    }
}
