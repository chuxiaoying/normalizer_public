interface ShapePort { void effect(int value); }
class Shape {
  @normalizer.annotations.External private final ShapePort port;
  Shape(ShapePort port) { this.port = port; }
  Object unused(boolean b) { return b ? port.effect(1) : 1; }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
