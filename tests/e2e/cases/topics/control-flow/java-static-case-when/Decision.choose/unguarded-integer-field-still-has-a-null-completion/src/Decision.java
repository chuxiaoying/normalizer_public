import normalizer.intrinsics.N;

class Box { int value; }
class Decision {
    static int choose(@N.Nullable Box box) {
        if (box.value > 1) return 5;
        return 9;
    }
}
