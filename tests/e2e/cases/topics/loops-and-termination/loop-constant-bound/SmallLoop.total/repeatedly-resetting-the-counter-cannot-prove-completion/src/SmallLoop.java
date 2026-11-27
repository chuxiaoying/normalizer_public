class SmallLoop {
    static int total() {
        int sum = 0;
        for (int i = 0; i < 3; i++) { if (i == 1) i = 0; sum += i; }
        return sum;
    }
}
