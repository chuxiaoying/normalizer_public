class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[]{(new int[]{2})[1], (new int[-1]).length}).length;
        return 7;
    }
}
