class Pair {
    int left;
    int right;

    Pair(int left) { this.left = left; }
    Pair(int left, int right) {
        int next = left + 3;
        super();
        this.left = next;
        this.right = right;
    }
}

class PairFactory {
    static Pair make(int value) {
        return new Pair(value, value + 1);
    }
}
