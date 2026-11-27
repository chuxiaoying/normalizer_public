class AliasCollection {
    static int total(int[] values, int next) {
        int[] alias = values;
        try {
            values[1] = next;
        } catch (ArithmeticException ignored) {}
        int total = 0;
        for (int value : alias) total += value;
        return total;
    }
}
