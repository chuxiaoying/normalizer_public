import normalizer.intrinsics.N;

class Box { int value; }
class Decision {
    static int choose(@N.Nullable Box box) {
        if (box == null) return 3;
        if (box.value < 2) return 9;
        return 5;
    }
}
