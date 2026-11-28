class Pair {
    int left;
    int right;

    Pair(int left) { this.left = left; }
    Pair(int left, int right) {
        this(left);
        this(right);
        this.right = right;
    }
}

class PairFactory {
    static Pair make(int value) {
        return new Pair(value, value + 1);
    }
}
