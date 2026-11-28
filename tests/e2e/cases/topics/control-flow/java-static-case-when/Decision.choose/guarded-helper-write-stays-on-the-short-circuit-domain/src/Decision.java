import normalizer.intrinsics.N;

class Box { int value; int other; }
class Decision {
    static int choose(@N.Nullable Box box) {
        if (box != null && pass(box)) return 5;
        if (box == null) return 3;
        return 9;
    }

    static boolean pass(Box box) {
        box.other = box.other + 1;
        return box.value > 1;
    }
}
