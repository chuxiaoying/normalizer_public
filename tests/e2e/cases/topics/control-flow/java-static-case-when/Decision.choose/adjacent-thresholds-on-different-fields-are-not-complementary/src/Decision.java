import normalizer.intrinsics.N;

class Box { int value; int other; }
class Decision {
    static int choose(@N.Nullable Box box) {
        if (box == null) return 3;
        if (box.value > 1) return 5;
        if (box.other < 2) return 9;
        return 7;
    }
}
