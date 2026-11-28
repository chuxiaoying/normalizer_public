interface Reader { int get(); }
class Receiver { int read() { return 9; } }
class Quote {
    static int run() {
        Receiver receiver = null;
        Reader invalid = receiver::missing;
        return 7;
    }
}
