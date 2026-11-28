package steps;

class Suite {
    static class Box {
        private Box() {}

        private int twice(int amount) {
            return amount * 2;
        }
    }

    static class Entry {
        static int read(int amount) {
            Box box = new Box();
            return box.twice(amount);
        }
    }
}
