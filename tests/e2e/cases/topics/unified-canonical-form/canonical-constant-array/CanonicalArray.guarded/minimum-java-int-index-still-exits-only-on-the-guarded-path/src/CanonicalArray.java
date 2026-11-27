class CanonicalArray {
    static int guarded(boolean fail) {
        if (fail) return (new int[]{2, 7})[-2147483648];
        return 7;
    }
}
