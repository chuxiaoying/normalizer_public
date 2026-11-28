import normalizer.intrinsics.N;

class Box { int value; }
class Decision {
    static int choose(@N.Nullable Box box) {
        if (box != null && box.value > 1) return 5;
        if (box == null) return 3;
        return 9;
    }
}
