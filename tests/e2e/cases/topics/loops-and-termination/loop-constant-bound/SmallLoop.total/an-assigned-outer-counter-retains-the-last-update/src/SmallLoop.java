class SmallLoop {
    static int total() {
        int i = 99; int sum = 0;
        for (i = 0; i < 3; i++) sum += i;
        return sum * 10 + i;
    }
}
