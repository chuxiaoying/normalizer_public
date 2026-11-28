class Box { int value; }
class Identity {
    static boolean sameIdentity(Object a, Box b) {
        return a == b;
    }
}
