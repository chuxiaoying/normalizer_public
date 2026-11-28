record Marker() {}
class Identity {
    static boolean sameIdentity() {
        Marker first = new Marker(); Marker second = first; return first == second;
    }
}
