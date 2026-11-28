interface Reader { int get(); }
class Receiver { int read() { return 9; } }
class Quote {
    static int run() {
        Receiver receiver = null;
        Reader reader = () -> receiver.read();
        return 7;
    }
}
