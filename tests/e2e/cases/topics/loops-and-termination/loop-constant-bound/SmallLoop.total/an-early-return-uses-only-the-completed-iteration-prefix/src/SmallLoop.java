class SmallLoop {
    static int total() {
        int sum = 0;
        for (int i = 0; i < 3; i++) { sum += i; if (i == 1) return sum + 10; }
        return sum;
    }
}
