class AliasCollection {
    static int total(int[] values, int next) {
        int[] alias = values;
        try {
            values[0] = next;
            throw new RuntimeException("checkpoint");
        } catch (RuntimeException ignored) {}
        int total = 0;
        for (int value : alias) total += value;
        return total;
    }
}
