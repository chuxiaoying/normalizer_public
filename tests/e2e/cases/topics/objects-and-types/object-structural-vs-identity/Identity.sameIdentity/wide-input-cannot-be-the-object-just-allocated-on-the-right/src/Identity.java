class Box { int value; }
class Identity {
    static boolean sameIdentity(Object a) {
        return a == new Box();
    }
}
