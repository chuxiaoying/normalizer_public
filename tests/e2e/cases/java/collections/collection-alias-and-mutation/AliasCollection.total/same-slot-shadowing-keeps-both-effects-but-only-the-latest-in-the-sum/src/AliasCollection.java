class AliasCollection {
    static int total(int[] values, int next) {
        int[] alias = values;
        try {
            values[0] = next;
            alias[0] = next + 1;
        } catch (ArrayIndexOutOfBoundsException ignored) {}
        int total = 0;
        for (int value : alias) total += value;
        return total;
    }
}
