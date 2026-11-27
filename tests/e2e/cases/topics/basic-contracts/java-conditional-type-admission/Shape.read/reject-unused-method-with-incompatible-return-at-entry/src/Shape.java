class Shape {
    static int unused() { return true; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
