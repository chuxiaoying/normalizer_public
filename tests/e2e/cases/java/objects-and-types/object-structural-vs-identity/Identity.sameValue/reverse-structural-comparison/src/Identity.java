class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return right.value == left.value;
    }

    static boolean sameIdentity(Box left, Box right) {
        return left == right;
    }
}
