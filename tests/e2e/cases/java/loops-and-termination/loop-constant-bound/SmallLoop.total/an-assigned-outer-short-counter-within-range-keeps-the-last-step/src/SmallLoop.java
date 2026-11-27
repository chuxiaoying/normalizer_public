class SmallLoop {
    static int total() {
        short i = 99; int sum = 0;
        for (i = 0; i < 3; i++) sum += i;
        return sum * 10 + i;
    }
}
