class SmallLoop {
    static int total() {
        int i = 0; int sum = 0;
        for (i = 0; i < 3; i++) { if (i == 1) i = 2; sum += i; }
        return sum;
    }
}
