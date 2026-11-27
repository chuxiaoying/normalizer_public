class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(Box right, Box left) {
        return left == right;
    }
}
