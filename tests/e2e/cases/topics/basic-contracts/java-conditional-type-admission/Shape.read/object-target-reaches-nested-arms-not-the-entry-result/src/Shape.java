class Shape {
    static java.lang.Object unused(boolean b) { return b ? (b ? 1 : true) : null; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
