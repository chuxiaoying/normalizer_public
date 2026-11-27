class SmallLoop {
    static long total(int[] items) {
        long sum = 0L;
        for (int v : items) sum += v;
        return sum;
    }
}
