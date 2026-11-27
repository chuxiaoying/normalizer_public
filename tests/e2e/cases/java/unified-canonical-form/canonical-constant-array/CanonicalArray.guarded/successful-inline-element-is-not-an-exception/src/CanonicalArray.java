class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[]{2, 7})[1];
        return 7;
    }
}
