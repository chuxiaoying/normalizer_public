class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(Box left, Box right, boolean chooseA, boolean chooseB) {
        Box a = left; if (chooseA) { a = right; } Box b = left; if (chooseB) { b = right; } return a == b;
    }
}
