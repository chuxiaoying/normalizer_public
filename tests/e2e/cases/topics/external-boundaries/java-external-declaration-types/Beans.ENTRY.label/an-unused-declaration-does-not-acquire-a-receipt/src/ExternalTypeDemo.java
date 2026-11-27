import normalizer.intrinsics.N;

class ExternalTypeDemo {
    @N.External
    private final Records port;
    ExternalTypeDemo(Records port) { this.port = port; }
    String label(int id) {
        return "offline";
    }
}
class Records {
    native Packet lookup(int id);
}
class Packet { boolean approved; String label; }

class Beans {
    static final ExternalTypeDemo ENTRY = new ExternalTypeDemo(new Records());
}
