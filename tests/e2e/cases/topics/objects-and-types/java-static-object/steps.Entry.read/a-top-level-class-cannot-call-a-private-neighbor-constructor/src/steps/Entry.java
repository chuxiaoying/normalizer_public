package steps;

class Box {
    private Box() {}

    int twice(int amount) {
        return amount * 2;
    }
}

class Entry {
    static int read(int amount) {
        Box box = new Box();
        return box.twice(amount);
    }
}
