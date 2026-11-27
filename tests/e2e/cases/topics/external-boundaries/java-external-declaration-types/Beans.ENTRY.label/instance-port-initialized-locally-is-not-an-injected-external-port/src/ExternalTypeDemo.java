import normalizer.intrinsics.N;

class ExternalTypeDemo {
    @N.External
    private final Records port = null;
    ExternalTypeDemo(Records port) {  }
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
