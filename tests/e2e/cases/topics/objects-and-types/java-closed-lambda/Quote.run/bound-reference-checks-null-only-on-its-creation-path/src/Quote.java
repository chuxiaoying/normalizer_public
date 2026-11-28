interface Reader { int get(); }
class Receiver { int read() { return 9; } }
class Quote {
    static int run(boolean active) {
        Receiver receiver = null;
        if (!active) return 7;
        Reader reader = receiver::read;
        return reader.get();
    }
}
