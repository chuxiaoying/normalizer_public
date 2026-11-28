class Box { int value; }
class Identity {
    static boolean sameIdentity(Object a) {
        return new Box() != a;
    }
}
