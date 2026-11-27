class Shape {
    static int unused(int n) { if (1) return n; return 0; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
