import normalizer.intrinsics.N;

interface ShapePort { void effect(int value); }
class Shape {
  @N.External private final ShapePort port;
  Shape(ShapePort port) { this.port = port; }
  void unused(boolean b) { if (b) port.effect(1); }
    static int read(boolean choose, int[] values, int value) {
        return choose ? value : 1;
    }
}
