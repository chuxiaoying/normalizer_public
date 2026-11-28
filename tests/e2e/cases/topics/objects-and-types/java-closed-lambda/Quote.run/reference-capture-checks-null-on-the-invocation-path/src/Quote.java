interface Reader { int get(); }
class Receiver { int read() { return 9; } }
class Quote {
    static int run(boolean active) {
        Receiver receiver = null;
        Reader reader = () -> receiver.read();
        if (active) return reader.get();
        return 7;
    }
}
