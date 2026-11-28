class Box { int value; }
class Identity {
    static boolean sameIdentity(Object b, Box a) {
        return a != b;
    }
}
