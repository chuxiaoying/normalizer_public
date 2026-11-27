class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(Box left, Box right, boolean replace) {
        Box alias = left; if (replace) { alias = null; } return alias == left;
    }
}
