import normalizer.intrinsics.N;

class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(@N.Nullable Box left, Box right) {
        return left != null;
    }
}
