import normalizer.intrinsics.N;

class ExternalTypeDemo {
    @N.External
    private final Records port;
    ExternalTypeDemo(Records port) { this.port = port; }
    java.lang.String label(int id) {
        Packet packet = port.lookup(id);
        return packet.label;
    }
}
class Records {
    native Packet lookup(int id);
}
class Packet { boolean approved; java.lang.String label; }

class Beans {
    static final ExternalTypeDemo ENTRY = new ExternalTypeDemo(new Records());
}
