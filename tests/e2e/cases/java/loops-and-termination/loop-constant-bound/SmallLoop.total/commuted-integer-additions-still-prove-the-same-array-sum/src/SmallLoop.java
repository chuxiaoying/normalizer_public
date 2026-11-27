class SmallLoop {
    static int total(int[] items) {
        int sum = 0;
        for (int v : items) { sum = (1 + v) + sum; }
        return sum;
    }
}
