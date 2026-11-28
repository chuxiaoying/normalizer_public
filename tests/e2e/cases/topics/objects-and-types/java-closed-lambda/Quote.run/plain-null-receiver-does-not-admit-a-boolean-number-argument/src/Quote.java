class Receiver { int read(int delta) { return delta + 9; } }
class Quote {
    static int run(boolean active) {
        Receiver receiver = null;
        return receiver.read(active);
    }
}
