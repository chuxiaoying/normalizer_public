class AliasCollection {
    static int total(int[] values, int next) {
        int[] alias = values;
        values[0] = next;
        int total = 0;
        for (int value : alias) total += value + 1;
        return total;
    }
}
