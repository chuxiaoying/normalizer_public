class Shape {
    static Object unused(boolean b) { return b ? 1 : true; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
