import normalizer.intrinsics.N;

class ExternalTypeDemo {
    @N.External
    private Records port;
    ExternalTypeDemo(Records port) { this.port = port; }
    String label(int id) {
        Packet packet = port.lookup(id);
        return packet.label;
    }
}
class Records {
    native Packet lookup(int id);
}
class Packet { boolean approved; String label; }

class Beans {
    static final ExternalTypeDemo ENTRY = new ExternalTypeDemo(new Records());
}
