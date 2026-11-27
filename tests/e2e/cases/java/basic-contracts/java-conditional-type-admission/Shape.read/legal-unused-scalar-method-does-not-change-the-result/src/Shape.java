class Shape {
    static int unused(int n) { if (n > 0) return n; return 0; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
