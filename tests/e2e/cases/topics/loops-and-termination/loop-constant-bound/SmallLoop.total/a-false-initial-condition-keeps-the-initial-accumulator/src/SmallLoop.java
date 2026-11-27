class SmallLoop {
    static int total() {
        int sum = 0;
        for (int i = 1; i < 0; i++) sum += i;
        return sum;
    }
}
