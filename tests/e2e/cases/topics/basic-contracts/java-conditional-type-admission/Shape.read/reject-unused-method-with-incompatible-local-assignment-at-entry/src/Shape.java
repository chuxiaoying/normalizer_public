class Shape {
    static int unused() { int n = 0; n = false; return n; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
