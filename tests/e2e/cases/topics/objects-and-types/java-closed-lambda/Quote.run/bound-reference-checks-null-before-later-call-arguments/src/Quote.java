interface Reader { int get(int value); }
class Receiver { int read(int value) { return value + 9; } }
class Quote {
    static int run() {
        Receiver receiver = null;
        Reader reader = receiver::read;
        int zero = 0;
        return reader.get(1 / zero);
    }
}
