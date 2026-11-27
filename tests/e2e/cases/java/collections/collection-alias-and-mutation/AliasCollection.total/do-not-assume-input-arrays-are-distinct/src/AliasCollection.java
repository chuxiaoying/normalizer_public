class AliasCollection {
    static int total(int[] values, int next, int[] other) {
        int[] alias = other;
        values[0] = next;
        int total = 0;
        for (int value : alias) total += value;
        return total;
    }
}
