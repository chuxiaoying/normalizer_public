class Receiver { int read(int delta) { return delta + 9; } }
class Quote {
    static int run(boolean active) {
        Receiver receiver = null;
        int zero = 0;
        if (active) return receiver.read(1 / zero);
        return receiver.read(2);
    }
}
