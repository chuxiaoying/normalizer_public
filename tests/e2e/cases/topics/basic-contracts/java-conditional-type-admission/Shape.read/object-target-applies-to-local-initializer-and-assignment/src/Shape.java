class Shape {
    static int unused(boolean b) { Object o = b ? 1 : true; o = b ? "yes" : 2; o = b ? true : false; o = b ? 2 : 3; return 4; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
