class CanonicalArray {
    static int guarded(boolean fail) {
        try { return (new int[]{2, 7})[2]; } catch (ArrayIndexOutOfBoundsException ignored) { return 17; }
    }
}
