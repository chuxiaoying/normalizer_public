class Box {
    int length;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.length == right.length;
    }

    static boolean sameIdentity(Box left, Box right) {
        return left == right;
    }
}
