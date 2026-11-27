class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[]{})[0];
        return 7;
    }
}
