class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(Box left, Box right) {
        Box first = new Box(); Box second = new Box(); first.value = 7; second.value = 7; return first == second;
    }
}
