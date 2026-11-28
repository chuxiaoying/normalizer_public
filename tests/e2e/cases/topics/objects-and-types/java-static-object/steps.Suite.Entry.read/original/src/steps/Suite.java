package steps;

class Suite {
    static class Box {
        private int value;

        Box(int value) {
            this.value = value;
        }
    }

    static class Entry {
        static int read(int amount) {
            Box box = new Box(amount);
            return box.value;
        }
    }
}
