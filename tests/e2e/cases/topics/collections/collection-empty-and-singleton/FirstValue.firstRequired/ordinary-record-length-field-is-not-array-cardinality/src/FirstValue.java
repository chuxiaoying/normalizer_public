class Box {
    int length;
    Box(int value) { this.length = value; }
}

class FirstValue {
    static int firstRequired(int[] values) {
        Box box = new Box(7); return box.length;
    }
}
