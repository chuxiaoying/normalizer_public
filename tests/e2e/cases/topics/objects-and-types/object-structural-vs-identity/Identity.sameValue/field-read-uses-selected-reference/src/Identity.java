class Box {
    int value;
    Box(int value) { this.value = value; }
}

class Identity {
    static int readSelected(boolean pickLeft) {
        Box left = new Box(2);
        Box right = new Box(3);
        Box selected = right;
        if (pickLeft) selected = left;
        return selected.value;
    }
}
