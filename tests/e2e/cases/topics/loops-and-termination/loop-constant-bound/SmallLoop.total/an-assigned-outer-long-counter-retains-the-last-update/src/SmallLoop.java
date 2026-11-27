class SmallLoop {
    static long total() {
        long i = 99L; long sum = 0L;
        for (i = 0L; i < 3L; i++) sum += i;
        return sum * 10L + i;
    }
}
