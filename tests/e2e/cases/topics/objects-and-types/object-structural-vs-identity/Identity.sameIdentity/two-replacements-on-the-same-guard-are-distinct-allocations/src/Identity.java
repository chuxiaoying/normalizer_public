class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(Box left, Box right, boolean replace) {
        Box a = left; if (replace) { a = new Box(); } Box b = left; if (replace) { b = new Box(); } return a == b;
    }
}
