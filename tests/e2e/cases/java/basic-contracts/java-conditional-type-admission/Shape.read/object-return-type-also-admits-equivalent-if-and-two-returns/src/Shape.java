class Shape {
    static Object unused(boolean b) { if (b) return 1; return true; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
