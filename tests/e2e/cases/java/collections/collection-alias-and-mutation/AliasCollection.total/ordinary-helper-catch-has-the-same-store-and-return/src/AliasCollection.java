class AliasCollection {
    static int changedTotal(int[] values, int next) {
        values[1] = next;
        return unchangedTotal(values);
    }

    static int unchangedTotal(int[] values) {
        int total = 0;
        for (int value : values) total += value;
        return total;
    }
    static int total(int[] values, int next) {
        try {
            return changedTotal(values, next);
        } catch (ArrayIndexOutOfBoundsException ignored) {
            return unchangedTotal(values);
        }
    }
}
