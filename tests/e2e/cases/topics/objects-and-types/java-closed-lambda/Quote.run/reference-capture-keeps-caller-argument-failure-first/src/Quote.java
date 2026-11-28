interface Reader { int get(int delta); }
class Receiver { int read(int delta) { return delta + 9; } }
class Quote {
    static int run(boolean active) {
        Receiver receiver = null;
        Reader reader = step -> receiver.read(step);
        int zero = 0;
        if (active) return reader.get(1 / zero);
        return reader.get(2);
    }
}
