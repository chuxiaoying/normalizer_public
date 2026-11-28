interface Reader { int get(int delta); }
class Receiver {
    int value;
    int read(int delta) { return this.value + delta; }
}
class Quote {
    static int run(int delta) {
        Receiver receiver = new Receiver();
        Receiver alias = receiver;
        Reader reader = receiver::read;
        receiver = new Receiver();
        receiver.value = 20;
        alias.value = 9;
        return reader.get(delta);
    }
}
