record Marker() {}
class Identity {
    static boolean sameIdentity() {
        return new Marker() == new Marker();
    }
}
