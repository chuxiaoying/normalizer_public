import normalizer.intrinsics.N;

class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(@N.Nullable Box prior, boolean p, boolean q) {
        Box x = new Box(); Box a = prior; if (p) { a = x; } Box b = prior; if (q) { b = x; } return a == b;
    }
}
