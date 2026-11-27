class Pair {
    int left = 7;
    int right;

    Pair(int left, int right) {
        this.left = this.left + left;
        this.right = right;
    }
}

class PairFactory {
    static Pair make(int value) {
        return new Pair(value, value + 1);
    }
}
