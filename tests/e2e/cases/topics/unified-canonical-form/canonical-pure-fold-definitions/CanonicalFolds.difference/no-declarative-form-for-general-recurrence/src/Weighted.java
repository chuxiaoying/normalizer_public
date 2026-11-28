class Weighted {
    static int run(int[] values, int initial) {
        int total = initial;
        for (int item : values) {
            total = total * 2 + item;
        }
        return total;
    }
}
