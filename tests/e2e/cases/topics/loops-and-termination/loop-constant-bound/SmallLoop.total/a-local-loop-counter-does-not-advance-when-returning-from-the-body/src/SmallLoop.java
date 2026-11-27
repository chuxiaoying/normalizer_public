class SmallLoop {
    static int total() {
        int i = 99; int sum = 0;
        for (i = 0; i < 3; i++) { sum += i; if (i == 1) return 10 * i + sum; }
        return sum;
    }
}
