class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value + 1;
    }

    static boolean sameIdentity(Box left, Box right) {
        return left == right;
    }
}
