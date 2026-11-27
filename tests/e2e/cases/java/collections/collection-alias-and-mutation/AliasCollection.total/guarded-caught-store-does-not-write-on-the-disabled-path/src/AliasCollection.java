class AliasCollection {
    static int total(int[] values, int next, boolean enabled) {
        int[] alias = values;
        try {
            if (enabled) { values[1] = next; }
        } catch (ArrayIndexOutOfBoundsException ignored) {}
        int total = 0;
        for (int value : alias) total += value;
        return total;
    }
}
