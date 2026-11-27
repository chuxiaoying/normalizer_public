class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[]{(new int[-1]).length, (new int[]{2})[1]}).length;
        return 7;
    }
}
