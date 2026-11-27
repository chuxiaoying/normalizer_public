class Adjustment {
    static int adjust(int base, int x, int y, boolean preferred) {
        long sum = (long) x + y;
        long difference = (long) x - y;
        boolean eligible = sum <= 10 && sum >= 10 && 2 * difference == 4;

        if ((eligible && preferred) || (eligible && !preferred)) {
            return (base + x) - x;
        }
        return 0;
    }
}
