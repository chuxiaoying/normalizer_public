class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[-1]).length;
        return 7;
    }
}
