class Shape {
  static Object unused(int n) { return n ? 1 : true; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
