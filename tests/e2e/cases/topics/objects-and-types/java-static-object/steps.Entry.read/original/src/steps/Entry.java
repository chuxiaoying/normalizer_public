package steps;

class Box {
    private int value;

    Box(int value) {
        this.value = value;
    }
}

class Entry {
    static int read(int amount) {
        Box box = new Box(amount);
        return box.value;
    }
}
